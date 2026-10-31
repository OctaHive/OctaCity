# OctaCity project goals

Build a self-hosted control plane for reproducible CI and Factory workflows.
The server owns durable orchestration, versioned configuration, admission,
budgets, evidence and recovery. Agents execute leased work in isolated,
bounded environments through installed and explicitly authorized tools.

Retain step states and results internally. Start each agent step in a separate
execution and provide only its selected immutable inputs, Project context and
accepted handoffs. A previous step's private investigation, credentials and
ambient configuration are not inherited by the next step.

Keep execution fenced, idempotent and recoverable after interruptions. Preserve
the exact source, configuration and evidence references accepted for each Run.
Expose useful status, retained results and editable versioned Project context
in the Console without making the browser an owner of execution state.
