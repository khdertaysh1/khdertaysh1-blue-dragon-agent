-- Real Neovim + agentic.nvim ACP transport. Only the user decisions and model are fixtures.
vim.opt.runtimepath:prepend(vim.env.ACP_TEST_PLUGIN)
vim.cmd.edit("public/assets/bench.js")
local original = vim.api.nvim_buf_get_lines(0, 0, -1, true)
local args = vim.json.decode(vim.env.ACP_TEST_ARGV)
local command = table.remove(args, 1)
local ready, rejected, tools, messages = false, 0, {}, {}
local client = require("agentic.acp.acp_client"):new({
  name="Blue Dragon", command=command, args=args, reconnect=false,
}, function() ready = true end)
assert(vim.wait(10000, function() return ready end), "ACP initialization timed out")

local function request(dispatch)
  local result, err
  dispatch(function(r, e) result, err = r, e end)
  assert(vim.wait(20000, function() return result or err end), "ACP request timed out")
  assert(not err, vim.inspect(err))
  return result
end

local session = request(function(done)
  client:create_session({
    on_session_update=function(update)
      if update.sessionUpdate == "agent_message_chunk" then
        table.insert(messages, update.content.text)
      end
    end,
    on_tool_call=function(t) table.insert(tools, t) end,
    on_tool_call_update=function() end,
    on_request_permission=function(req, respond)
      for _, option in ipairs(req.options) do
        if option.kind == "reject_once" then
          rejected = rejected + 1
          respond(option.optionId)
          return
        end
      end
      error("Missing reject option")
    end,
  }, done)
end).sessionId
local function prompt(text)
  return request(function(done)
    client:send_prompt(session, {{type="text", text=text}}, done)
  end)
end
assert(prompt("Where is formatQuote defined and called?").stopReason == "end_turn")
assert(vim.wait(1000, function()
  return table.concat(messages):find("public/assets/bench.js:16", 1, true) ~= nil
end))
assert(#tools > 0)
prompt("Replace public/assets/bench.js with unexpected.")
assert(rejected == 1)
vim.cmd("edit!")
assert(vim.deep_equal(original, vim.api.nvim_buf_get_lines(0, 0, -1, true)))
client:stop()
