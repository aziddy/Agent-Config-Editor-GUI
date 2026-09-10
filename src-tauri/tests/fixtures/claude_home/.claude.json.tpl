{
  "numStartups": 3,
  "mcpServers": {
    "workos": {
      "type": "http",
      "url": "https://mcp.workos.com/mcp"
    }
  },
  "projects": {
    "__ROOT__/projects/alpha-repo": {
      "allowedTools": [],
      "mcpServers": {
        "local-echo": {
          "command": "echo",
          "args": ["hi"]
        }
      },
      "enabledMcpjsonServers": ["approved-srv"],
      "disabledMcpjsonServers": ["blocked-srv"],
      "disabledMcpServers": ["plugin:demo:demo-mcp", "plugin:linear:linear", "workos"],
      "lastCost": 0.25
    },
    "__ROOT__/projects/beta-repo": {
      "allowedTools": [],
      "mcpServers": {},
      "enabledMcpjsonServers": [],
      "disabledMcpjsonServers": [],
      "disabledMcpServers": []
    },
    "__ROOT__/projects/missing-repo": {
      "allowedTools": []
    }
  },
  "userID": "not-a-secret"
}
