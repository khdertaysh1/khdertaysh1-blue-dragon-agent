//! Visual-only shaping for terminals without Arabic shaping or bidi support.
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use wezterm_bidi::{
    bidi_class_for_char, BidiClass, BidiContext, Direction, ParagraphDirectionHint,
};

// Single-codepoint Arabic presentation forms from Unicode decomposition mappings.
// Columns: isolated, final, initial, medial. Ligatures requiring multiple bases
// are intentionally omitted so each original grapheme remains editable.
fn forms(ch: char) -> Option<[char; 4]> {
    Some(match ch {
        '\u{621}' => ['\u{fe80}', '\u{621}', '\u{621}', '\u{621}'],
        '\u{622}' => ['\u{fe81}', '\u{fe82}', '\u{622}', '\u{622}'],
        '\u{623}' => ['\u{fe83}', '\u{fe84}', '\u{623}', '\u{623}'],
        '\u{624}' => ['\u{fe85}', '\u{fe86}', '\u{624}', '\u{624}'],
        '\u{625}' => ['\u{fe87}', '\u{fe88}', '\u{625}', '\u{625}'],
        '\u{626}' => ['\u{fe89}', '\u{fe8a}', '\u{fe8b}', '\u{fe8c}'],
        '\u{627}' => ['\u{fe8d}', '\u{fe8e}', '\u{627}', '\u{627}'],
        '\u{628}' => ['\u{fe8f}', '\u{fe90}', '\u{fe91}', '\u{fe92}'],
        '\u{629}' => ['\u{fe93}', '\u{fe94}', '\u{629}', '\u{629}'],
        '\u{62a}' => ['\u{fe95}', '\u{fe96}', '\u{fe97}', '\u{fe98}'],
        '\u{62b}' => ['\u{fe99}', '\u{fe9a}', '\u{fe9b}', '\u{fe9c}'],
        '\u{62c}' => ['\u{fe9d}', '\u{fe9e}', '\u{fe9f}', '\u{fea0}'],
        '\u{62d}' => ['\u{fea1}', '\u{fea2}', '\u{fea3}', '\u{fea4}'],
        '\u{62e}' => ['\u{fea5}', '\u{fea6}', '\u{fea7}', '\u{fea8}'],
        '\u{62f}' => ['\u{fea9}', '\u{feaa}', '\u{62f}', '\u{62f}'],
        '\u{630}' => ['\u{feab}', '\u{feac}', '\u{630}', '\u{630}'],
        '\u{631}' => ['\u{fead}', '\u{feae}', '\u{631}', '\u{631}'],
        '\u{632}' => ['\u{feaf}', '\u{feb0}', '\u{632}', '\u{632}'],
        '\u{633}' => ['\u{feb1}', '\u{feb2}', '\u{feb3}', '\u{feb4}'],
        '\u{634}' => ['\u{feb5}', '\u{feb6}', '\u{feb7}', '\u{feb8}'],
        '\u{635}' => ['\u{feb9}', '\u{feba}', '\u{febb}', '\u{febc}'],
        '\u{636}' => ['\u{febd}', '\u{febe}', '\u{febf}', '\u{fec0}'],
        '\u{637}' => ['\u{fec1}', '\u{fec2}', '\u{fec3}', '\u{fec4}'],
        '\u{638}' => ['\u{fec5}', '\u{fec6}', '\u{fec7}', '\u{fec8}'],
        '\u{639}' => ['\u{fec9}', '\u{feca}', '\u{fecb}', '\u{fecc}'],
        '\u{63a}' => ['\u{fecd}', '\u{fece}', '\u{fecf}', '\u{fed0}'],
        '\u{641}' => ['\u{fed1}', '\u{fed2}', '\u{fed3}', '\u{fed4}'],
        '\u{642}' => ['\u{fed5}', '\u{fed6}', '\u{fed7}', '\u{fed8}'],
        '\u{643}' => ['\u{fed9}', '\u{feda}', '\u{fedb}', '\u{fedc}'],
        '\u{644}' => ['\u{fedd}', '\u{fede}', '\u{fedf}', '\u{fee0}'],
        '\u{645}' => ['\u{fee1}', '\u{fee2}', '\u{fee3}', '\u{fee4}'],
        '\u{646}' => ['\u{fee5}', '\u{fee6}', '\u{fee7}', '\u{fee8}'],
        '\u{647}' => ['\u{fee9}', '\u{feea}', '\u{feeb}', '\u{feec}'],
        '\u{648}' => ['\u{feed}', '\u{feee}', '\u{648}', '\u{648}'],
        '\u{649}' => ['\u{feef}', '\u{fef0}', '\u{fbe8}', '\u{fbe9}'],
        '\u{64a}' => ['\u{fef1}', '\u{fef2}', '\u{fef3}', '\u{fef4}'],
        '\u{671}' => ['\u{fb50}', '\u{fb51}', '\u{671}', '\u{671}'],
        '\u{677}' => ['\u{fbdd}', '\u{677}', '\u{677}', '\u{677}'],
        '\u{679}' => ['\u{fb66}', '\u{fb67}', '\u{fb68}', '\u{fb69}'],
        '\u{67a}' => ['\u{fb5e}', '\u{fb5f}', '\u{fb60}', '\u{fb61}'],
        '\u{67b}' => ['\u{fb52}', '\u{fb53}', '\u{fb54}', '\u{fb55}'],
        '\u{67e}' => ['\u{fb56}', '\u{fb57}', '\u{fb58}', '\u{fb59}'],
        '\u{67f}' => ['\u{fb62}', '\u{fb63}', '\u{fb64}', '\u{fb65}'],
        '\u{680}' => ['\u{fb5a}', '\u{fb5b}', '\u{fb5c}', '\u{fb5d}'],
        '\u{683}' => ['\u{fb76}', '\u{fb77}', '\u{fb78}', '\u{fb79}'],
        '\u{684}' => ['\u{fb72}', '\u{fb73}', '\u{fb74}', '\u{fb75}'],
        '\u{686}' => ['\u{fb7a}', '\u{fb7b}', '\u{fb7c}', '\u{fb7d}'],
        '\u{687}' => ['\u{fb7e}', '\u{fb7f}', '\u{fb80}', '\u{fb81}'],
        '\u{688}' => ['\u{fb88}', '\u{fb89}', '\u{688}', '\u{688}'],
        '\u{68c}' => ['\u{fb84}', '\u{fb85}', '\u{68c}', '\u{68c}'],
        '\u{68d}' => ['\u{fb82}', '\u{fb83}', '\u{68d}', '\u{68d}'],
        '\u{68e}' => ['\u{fb86}', '\u{fb87}', '\u{68e}', '\u{68e}'],
        '\u{691}' => ['\u{fb8c}', '\u{fb8d}', '\u{691}', '\u{691}'],
        '\u{698}' => ['\u{fb8a}', '\u{fb8b}', '\u{698}', '\u{698}'],
        '\u{6a4}' => ['\u{fb6a}', '\u{fb6b}', '\u{fb6c}', '\u{fb6d}'],
        '\u{6a6}' => ['\u{fb6e}', '\u{fb6f}', '\u{fb70}', '\u{fb71}'],
        '\u{6a9}' => ['\u{fb8e}', '\u{fb8f}', '\u{fb90}', '\u{fb91}'],
        '\u{6ad}' => ['\u{fbd3}', '\u{fbd4}', '\u{fbd5}', '\u{fbd6}'],
        '\u{6af}' => ['\u{fb92}', '\u{fb93}', '\u{fb94}', '\u{fb95}'],
        '\u{6b1}' => ['\u{fb9a}', '\u{fb9b}', '\u{fb9c}', '\u{fb9d}'],
        '\u{6b3}' => ['\u{fb96}', '\u{fb97}', '\u{fb98}', '\u{fb99}'],
        '\u{6ba}' => ['\u{fb9e}', '\u{fb9f}', '\u{6ba}', '\u{6ba}'],
        '\u{6bb}' => ['\u{fba0}', '\u{fba1}', '\u{fba2}', '\u{fba3}'],
        '\u{6be}' => ['\u{fbaa}', '\u{fbab}', '\u{fbac}', '\u{fbad}'],
        '\u{6c0}' => ['\u{fba4}', '\u{fba5}', '\u{6c0}', '\u{6c0}'],
        '\u{6c1}' => ['\u{fba6}', '\u{fba7}', '\u{fba8}', '\u{fba9}'],
        '\u{6c5}' => ['\u{fbe0}', '\u{fbe1}', '\u{6c5}', '\u{6c5}'],
        '\u{6c6}' => ['\u{fbd9}', '\u{fbda}', '\u{6c6}', '\u{6c6}'],
        '\u{6c7}' => ['\u{fbd7}', '\u{fbd8}', '\u{6c7}', '\u{6c7}'],
        '\u{6c8}' => ['\u{fbdb}', '\u{fbdc}', '\u{6c8}', '\u{6c8}'],
        '\u{6c9}' => ['\u{fbe2}', '\u{fbe3}', '\u{6c9}', '\u{6c9}'],
        '\u{6cb}' => ['\u{fbde}', '\u{fbdf}', '\u{6cb}', '\u{6cb}'],
        '\u{6cc}' => ['\u{fbfc}', '\u{fbfd}', '\u{fbfe}', '\u{fbff}'],
        '\u{6d0}' => ['\u{fbe4}', '\u{fbe5}', '\u{fbe6}', '\u{fbe7}'],
        '\u{6d2}' => ['\u{fbae}', '\u{fbaf}', '\u{6d2}', '\u{6d2}'],
        '\u{6d3}' => ['\u{fbb0}', '\u{fbb1}', '\u{6d3}', '\u{6d3}'],
        _ => return None,
    })
}

fn uses_bidi(s: &str) -> bool {
    !s.is_ascii()
        && s.chars().any(|ch| {
            matches!(
                bidi_class_for_char(ch),
                BidiClass::ArabicLetter | BidiClass::RightToLeft
            )
        })
}

fn joining(ch: char) -> (bool, bool) {
    if matches!(ch, '\u{640}' | '\u{200d}') {
        return (true, true);
    }
    forms(ch).map_or((false, false), |f| (f[1] != ch, f[2] != ch))
}

fn shape(chars: &[char]) -> Vec<char> {
    let mut out = chars.to_vec();
    let mut previous = None;
    let mut significant = chars
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, ch)| bidi_class_for_char(*ch) != BidiClass::NonspacingMark)
        .peekable();
    while let Some((i, ch)) = significant.next() {
        if let Some(f) = forms(ch) {
            let (to_previous, to_next) = joining(ch);
            let connects_previous = to_previous && previous.is_some_and(|p| joining(p).1);
            let connects_next = to_next && significant.peek().is_some_and(|(_, n)| joining(*n).0);
            out[i] = f[usize::from(connects_previous) + 2 * usize::from(connects_next)];
        }
        previous = Some(ch);
    }
    out
}

fn mirror(ch: char) -> char {
    match ch {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        _ => ch,
    }
}

struct Cell {
    logical: usize,
    text: String,
    rtl: bool,
}

fn visual_cells(s: &str) -> Vec<Cell> {
    let graphemes = s
        .graphemes(true)
        .map(|g| {
            if g.chars().all(char::is_control) {
                " "
            } else {
                g
            }
        })
        .collect::<Vec<_>>();
    if !uses_bidi(s) {
        return graphemes
            .iter()
            .enumerate()
            .map(|(logical, g)| Cell {
                logical,
                text: (*g).into(),
                rtl: false,
            })
            .collect();
    }
    let chars: Vec<_> = graphemes.iter().flat_map(|g| g.chars()).collect();
    let shaped = shape(&chars);
    let mut offsets = Vec::with_capacity(graphemes.len() + 1);
    let mut by_char = Vec::with_capacity(chars.len());
    for (index, g) in graphemes.iter().enumerate() {
        offsets.push(by_char.len());
        by_char.resize(by_char.len() + g.chars().count(), index);
    }
    offsets.push(chars.len());
    let mut bidi = BidiContext::new();
    bidi.resolve_paragraph(&chars, ParagraphDirectionHint::AutoLeftToRight);
    let mut seen = vec![false; graphemes.len()];
    let mut cells = Vec::with_capacity(graphemes.len());
    for run in bidi.reordered_runs(0..chars.len()) {
        let rtl = run.direction == Direction::RightToLeft;
        for i in run.indices {
            let logical = by_char[i];
            if std::mem::replace(&mut seen[logical], true) {
                continue;
            }
            let text = shaped[offsets[logical]..offsets[logical + 1]].iter().copied()
                .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
                .map(|c| if rtl { mirror(c) } else { c }).collect();
            cells.push(Cell { logical, text, rtl });
        }
    }
    cells
}

pub(crate) fn visual_line(s: &str) -> String {
    if !uses_bidi(s) {
        return s.to_owned();
    }
    visual_cells(s).into_iter().map(|cell| cell.text).collect()
}

fn caret_positions(cells: &[Cell], count: usize) -> Vec<usize> {
    let mut positions = vec![None; count + 1];
    for (i, cell) in cells.iter().enumerate() {
        positions[cell.logical] = Some(i + usize::from(cell.rtl));
        if cell.logical + 1 == count {
            positions[count] = Some(i + usize::from(!cell.rtl));
        }
    }
    let mut previous = 0;
    positions
        .into_iter()
        .map(|pos| {
            previous = pos.unwrap_or(previous);
            previous
        })
        .collect()
}

pub(crate) fn move_caret(input: &str, cursor: usize, right: bool) -> usize {
    let count = input.graphemes(true).count();
    if !uses_bidi(input) {
        return if right {
            (cursor + 1).min(count)
        } else {
            cursor.saturating_sub(1)
        };
    }
    let cells = visual_cells(input);
    let positions = caret_positions(&cells, count);
    let current = positions[cursor.min(count)];
    positions
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, p)| if right { *p > current } else { *p < current })
        .min_by_key(|(i, p)| (p.abs_diff(current), i.abs_diff(cursor)))
        .map_or(cursor, |(i, _)| i)
}

pub(crate) fn visible_input(input: &str, cursor: usize, width: usize) -> String {
    let cells = visual_cells(input);
    let count = input.graphemes(true).count();
    let pos = caret_positions(&cells, count)[cursor.min(count)].min(cells.len());
    let mut start = pos;
    let mut used: usize = 1;
    while start > 0 {
        let size = cells[start - 1].text.width();
        if used + size > width {
            break;
        }
        used += size;
        start -= 1;
    }
    let mut out = String::from(" > ");
    for cell in &cells[start..pos] {
        out.push_str(&cell.text);
    }
    out.push('▌');
    for cell in &cells[pos..] {
        let size = cell.text.width();
        if used + size > width {
            break;
        }
        used += size;
        out.push_str(&cell.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_reorders_arabic_without_moving_marks_to_the_wrong_letters() {
        assert_eq!(visual_line("سلام"), "\u{fee1}\u{fe8e}\u{fee0}\u{feb3}");
        assert_eq!(visual_line("سَلامُ"), "\u{fee1}ُ\u{fe8e}\u{fee0}\u{feb3}َ");
        let mixed = visual_line("نتيجة (ABC 123) جاهزة");
        assert!(mixed.contains("(ABC 123)"), "{mixed}");
        assert_eq!(visual_line("read_file x=12 👩‍💻"), "read_file x=12 👩‍💻");
    }

    #[test]
    fn moves_right_to_left_caret_and_does_not_cut_graphemes_on_resize() {
        assert_eq!(move_caret("سلام", 4, true), 3);
        assert_eq!(move_caret("سلام", 3, false), 4);
        assert_eq!(move_caret("سلام", 4, false), 4);
        assert!(visible_input("سلام", 4, 5).starts_with(" > ▌"));
        assert_eq!(visible_input("سَ", 1, 2), " > ▌\u{feb1}َ");
        for mutating_text in ["سَ\r\nُa", "ع\u{200d}ر", "ش\u{202e}abc\u{202c}", "ن 👩‍💻 ٢٣"]
        {
            for index in 0..=mutating_text.graphemes(true).count() {
                assert!(visible_input(mutating_text, index, 4).contains('▌'));
            }
        }
    }
}
