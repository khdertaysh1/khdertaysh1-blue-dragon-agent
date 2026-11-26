//! Small offline bilingual concept index for engineering-session retrieval.
//! The vocabulary is intentionally inspectable. It stores no vectors or raw tool bodies.
use serde_json::Value;
use std::{collections::HashMap, sync::OnceLock};

const GROUPS: &[&[&str]] = &[
    &[
        "quantity",
        "qty",
        "count",
        "amount",
        "quantityfield",
        "كمية",
        "الكمية",
        "عدد",
        "العدد",
    ],
    &[
        "field",
        "input",
        "form",
        "حقل",
        "الحقل",
        "خانة",
        "الخانة",
        "نموذج",
        "النموذج",
    ],
    &[
        "fill",
        "filled",
        "filling",
        "populate",
        "typed",
        "enter",
        "تعبئة",
        "عبينا",
        "تعباية",
        "عبي",
        "نغيره",
        "نغيرها",
    ],
    &[
        "click",
        "clicked",
        "press",
        "pressed",
        "tap",
        "نقر",
        "نقرة",
        "ضغط",
        "ضغطنا",
        "كبسة",
    ],
    &[
        "network",
        "request",
        "requests",
        "شبكة",
        "الشبكة",
        "طلبات",
        "الطلبات",
    ],
    &["response", "reply", "استجابة", "الاستجابة", "رد", "الرد"],
    &["save", "saved", "stored", "حفظ", "حفظنا", "محفوظ", "تخزين"],
    &[
        "file",
        "files",
        "artifact",
        "ملف",
        "ملفات",
        "الملف",
        "الملفات",
    ],
    &[
        "test",
        "tests",
        "unittest",
        "اختبار",
        "اختبارات",
        "الاختبار",
        "الاختبارات",
        "فحص",
    ],
    &[
        "error",
        "failed",
        "failure",
        "fail",
        "فشل",
        "فشلت",
        "خطأ",
        "الخطأ",
        "غلط",
    ],
    &["browser", "tab", "متصفح", "المتصفح", "تبويب", "التبويب"],
    &[
        "evaluate",
        "read",
        "value",
        "reading",
        "قيمة",
        "القيمة",
        "قيمته",
        "قرأنا",
        "قراءة",
    ],
    &["import", "imports", "imported", "استيراد", "استوردنا"],
    &[
        "definition",
        "defined",
        "declaration",
        "تعريف",
        "التعريف",
        "مكتوبة",
    ],
    &["function", "functionality", "دالة", "الدالة", "دوال"],
    &["timeout", "expired", "مهلة", "المهلة"],
];
const NUMBERS: &[(&str, &str)] = &[
    ("zero", "صفر"),
    ("one", "واحد"),
    ("two", "اثنين"),
    ("three", "ثلاثة"),
    ("four", "اربعة"),
    ("five", "خمسة"),
    ("six", "ستة"),
    ("seven", "سبعة"),
    ("eight", "ثمانية"),
    ("nine", "تسعة"),
    ("ten", "عشرة"),
];
const NUM_START: usize = GROUPS.len();
pub fn features(text: &str) -> u64 {
    let mut mask = 0;
    // Ignore Arabic vowel marks and normalize hamza and Eastern Arabic digits.
    let clean: String = text
        .chars()
        .filter_map(|c| match c {
            'ـ' | '\u{064b}'..='\u{065f}' => None,
            'أ' | 'إ' | 'آ' => Some('ا'),
            '٠'..='٩' => char::from_digit(c as u32 - '٠' as u32, 10),
            _ => Some(c),
        })
        .collect();
    static TERMS: OnceLock<HashMap<String, u64>> = OnceLock::new();
    let terms = TERMS.get_or_init(|| {
        let mut map = HashMap::new();
        for (i, group) in GROUPS.iter().enumerate() {
            for word in *group {
                map.insert(word.replace(['أ', 'إ', 'آ'], "ا"), 1 << i);
            }
        }
        for (i, (en, ar)) in NUMBERS.iter().enumerate() {
            for word in [en.to_string(), ar.to_string(), i.to_string()] {
                map.insert(word, 1 << (NUM_START + i));
            }
        }
        map
    });
    for token in clean
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        if let Some(bits) = terms
            .get(token)
            .or_else(|| token.strip_prefix('و').and_then(|word| terms.get(word)))
        {
            mask |= bits;
        }
    }
    mask
}

/// Return a byte offset in the lowercased source, as expected by the excerpt renderer.
pub fn first_hit(text: &str, wanted: u64) -> usize {
    let mut offset = 0;
    for part in text.split_inclusive(char::is_whitespace) {
        if features(part) & wanted != 0 {
            return offset;
        }
        offset += part.len();
    }
    0
}

pub fn similarity(query: u64, row: u64) -> Option<f64> {
    let overlap = query & row;
    let numeric = query >> NUM_START;
    if overlap.count_ones() < 2
        || overlap & ((1 << NUM_START) - 1) == 0
        || (numeric != 0 && (overlap >> NUM_START) != numeric)
    {
        return None;
    }
    Some(
        2.0 * f64::from(overlap.count_ones()) / f64::from(query.count_ones())
            + 0.25 * f64::from(overlap.count_ones()),
    )
}

// Expose only small counts from a direct property read on a quantity control.
// Never index arbitrary evaluated strings, typed values, or expression bodies.
pub fn observed_count(args: &Value, result: &Value) -> Option<String> {
    if args["action"] != "evaluate" || result["ok"] != true {
        return None;
    }
    let expr = args["expression"].as_str()?.trim().trim_end_matches(';');
    let arg = expr
        .strip_prefix("document.querySelector(")?
        .strip_suffix(").value")?;
    let quote = arg.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let selector = arg.strip_prefix(quote)?.strip_suffix(quote)?;
    let name = selector.strip_prefix('#')?;
    if !matches!(name, "quantity" | "qty" | "count" | "amount") {
        return None;
    }
    let v = &result["result"];
    let count: u64 = if let Some(s) = v.as_str() {
        s.parse().ok()?
    } else {
        v.as_u64()?
    };
    (count <= 10).then(|| format!("{selector} value {count}"))
}

pub fn past_question(prompt: &str) -> bool {
    let lower = prompt.trim().to_lowercase().replace('ّ', "");
    [
        "which session ",
        "in which session ",
        "بأي جلسة ",
        "باي جلسة ",
        "بأي محادثة ",
        "باي محادثة ",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
        && !lower.contains(['\n', ';', '؛'])
        && !lower.split(|c: char| !c.is_alphanumeric()).any(|s| {
            matches!(
                s,
                "now" | "currently" | "then" | "هلق" | "الان" | "الآن" | "وبعدين"
            )
        })
}
