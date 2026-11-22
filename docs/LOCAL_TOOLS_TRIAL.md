# Local review trial

In chat or the terminal UI, /inventory EVIDENCE_ID [OFFSET] shows eight unique complete URLs from a saved network capture. It reports partial URL counts and eviction counters separately. Resource bodies are not opened. /network also includes captured response statuses matched by request ID and exact URL, including responses outside the selected page.

Use /clipboard copy TEXT and /clipboard paste with Termux:API, pbcopy/pbpaste, wl-copy/wl-paste or xclip. Clipboard output is local. Desktop clipboard and search commands are explicit user commands; they are unavailable in read-only mode.

For /websearch WORDS, set BLUE_DRAGON_SEARCH_URL to your SearXNG /search endpoint with JSON enabled. The query goes to that configured endpoint. Search returns up to ten result URLs, titles and snippets. It does not open those results.

Use /sessions WORDS to find previous conversations and /conversation-search WORDS to find turns in this conversation. From a shell, blue-dragon sessions WORDS --open offers to reopen a match in its workspace.

Reopening a conversation displays its recorded goal, status, stop reason, next plan step and latest observation when available. In chat and the TUI, type /checkpoint or وين وقفنا؟ to read this summary locally. At startup, + shows the next five conversations in the current workspace.

A saved /network page now exposes recorded MIME and bounded policy indicators. The coverage field states whether raw headers and request/response bodies were examined by this view. Historical network pagination counts progress when distinct pages succeed. Repeated identical pages still stop.
