# Upstream reference
AlphaCode: https://github.com/dragonked2/alphacode
Reference snapshot: 7b325dfdb73066eaea372b7b39bf125effa2ae72.
Copyright (c) 2025–2026 Ali Essam. MIT license reproduced in AlphaCode-LICENSE.

Blue Dragon's workbench uses AlphaCode as an architectural reference for independent modules (streamed turns, planning, approved tools). The small JsonRpcRequest envelope in src/mcp_protocol.rs is adapted from src/alphacode_base/mcp/protocol.rs at that snapshot. Other implementations in this revision are Blue Dragon code. There is no AlphaCode hosted model endpoint or account flow bundled.
