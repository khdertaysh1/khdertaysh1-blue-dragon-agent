# Upstream reference
AlphaCode: https://github.com/dragonked2/alphacode
Reference snapshot: 7b325dfdb73066eaea372b7b39bf125effa2ae72.
Copyright (c) 2025–2026 Ali Essam. MIT license reproduced in AlphaCode-LICENSE.

Blue Dragon's workbench uses AlphaCode as an architectural reference for independent modules (streamed turns, planning, approved tools). The small JsonRpcRequest envelope in src/mcp_protocol.rs is adapted from src/alphacode_base/mcp/protocol.rs at that snapshot. Other implementations in this revision are Blue Dragon code. There is no AlphaCode hosted model endpoint or account flow bundled.

# Source-index dependencies
Tree-sitter 0.25.10 and tree-sitter-language 0.1.9: https://github.com/tree-sitter/tree-sitter (MIT).
JavaScript grammar 0.25.0: https://github.com/tree-sitter/tree-sitter-javascript (MIT).
TypeScript/TSX grammar 0.23.2: https://github.com/tree-sitter/tree-sitter-typescript (MIT).
Python grammar 0.25.0: https://github.com/tree-sitter/tree-sitter-python (MIT).
Rust grammar 0.24.2: https://github.com/tree-sitter/tree-sitter-rust (MIT).
sha2 0.10.9: https://github.com/RustCrypto/hashes (MIT option).
ignore 0.4.34: https://github.com/BurntSushi/ripgrep (MIT option).
Their license texts are reproduced in the corresponding *-LICENSE files and included with binary releases. They are parser/search libraries, not additional adapted AlphaCode components.
