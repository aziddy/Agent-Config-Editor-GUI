version = 1
id = "one-active"
kind = "cron"
name = "🚀 One active"
prompt = "Line one.\n\nLine `two` with code.\n- bullet"
status = "ACTIVE"
rrule = "RRULE:FREQ=WEEKLY;BYHOUR=1;BYMINUTE=15;BYDAY=SU,MO,TU,WE,TH,FR,SA"
model = "gpt-6-astra"
reasoning_effort = "high"
execution_environment = "worktree"
target = { type = "project", project_id = "local-0059b83a63155ebe17679fb4cf0c3f55" }
cwds = ["__ROOT__/projects/repo a"]
created_at = 1778733369975
updated_at = 1789065184555
