# Bounded MCP resource context

Blue Dragon supports named stdio and Streamable HTTP endpoints. Supply a configuration using `--mcp-config servers.json`. For example:

```json
{"servers":[{"name":"docs","url":"http://127.0.0.1:8765/mcp"}]}
```

Start the MCP server separately, then enter `/connect docs`. The endpoint must speak MCP. Remote servers require HTTPS; loopback also accepts plain HTTP. Set optional `bearer_env` to the name of an environment variable holding an access token. HTTP endpoints use JSON or SSE replies to POST, negotiate the protocol version, carry the server-issued session ID, and disable redirects and request replay. The older separate SSE endpoint transport and OAuth discovery are outside this implementation.

Use `/mcp prompts docs` to list templates, and `/mcp prompts docs CURSOR` for the next page. To preview a listed text prompt with its declared arguments:

```
/mcp prompt docs lesson {"topic":"أساسيات الشبكات"}
```

The command displays external messages with their roles and returns them as untrusted data. It does not insert them into the agent's instruction history.

After the user explicitly connects a configured server using `/connect NAME`, the root agent may call `mcp_resource_context` with `action=list` or `action=read` and `server`. Every list/read requires separate approval. Reads accept only exact URIs returned by the current connection's catalog; pagination accepts only the previously returned cursor. An initial list replaces the catalog. Resource URIs are opaque server identifiers: the client never opens them as files or fetches them over HTTP. The server runs with its own process/credential permissions, not browser scope or an OS sandbox. Child agents cannot use MCP.

Bounds: 32 entries/page, 128 distinct URIs/server, eight pages, 2048-byte URIs without embedded credentials/control characters, 256-byte names/cursors, and bounded displayed descriptions. Text reads require 1–8 blocks matching the requested URI, at most 64 KiB of UTF-8 text total. Binary content, URI substitution, repeated cursors, duplicate URIs and malformed/oversized data are rejected. No partial rejected content is returned; validation failure clears the catalog. Accepted text includes content hashes and an explicit untrusted-context label, never native security verification or authority for following embedded instructions.

A transport failure disconnects the server, clears tools/catalog and reports unknown without reconnect/replay. A process restart has no connected clients/catalog, so saved conversation URIs do not authorize new requests. Each explicit read is a new request; server freshness/cache semantics are unverified. Resource URI templates, subscriptions and automatic catalog refresh remain unsupported. Listing is discovery, not proof of permission or availability; the server must enforce its own access controls.

Validation uses a real synthetic stdio resource-only server and a loopback model with the actual CLI. It covers approvals, pagination, unlisted-URI rejection before RPC, wrong-resource/binary/oversized rejection, disconnect without replay and restart without reconnect. No external service account, production credentials or live provider is used. These tests do not establish Alpha parity or real integration behavior for every third-party MCP server.

Independent local review of `c89d6f8`/`3fd1e2f` subsequently passed within its inspected scope: approvals, URI/size boundaries, stdio isolation and restart behavior. Accepted text is not automatically stripped of secrets; untrusted labels do not guarantee prompt-injection immunity. The server retains its own permissions, and no live provider/service validation was performed.
