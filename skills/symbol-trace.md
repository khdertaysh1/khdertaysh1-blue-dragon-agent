---
auto-scope: workspace-read
match: وين مكتوبة {symbol} ومين بيناديها
match: وين تعريف {symbol} ووين بتنستدعى
match: Where is {symbol} defined and called
match: Find the definition and call sites of {symbol}
description: Locate a JS/TS function's definition and call sites across files in the current project. وين مكتوبة الدالة ومين بيناديها بالمشروع؟
---
Use code_search with kind="bindings" and the requested identifier. Cite the definition and call locations separately using their actual one-based file lines. Include the module chain if it was observed. Report ambiguous imports or missing matches as returned by the index.
