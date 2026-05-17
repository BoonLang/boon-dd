# Ply Renderer Review: dependency-boundary

Review Cargo metadata and crate boundaries. Pass only if active renderer crates depend on ply-engine rather than app_window, boon_backend_wgpu, shader build crates, or repo-owned custom browser rendering backends.

Required report schema: reviewer, model, git_commit, deterministic_report_sha256, prompt_file, prompt_sha256, commands_run, files_examined, deterministic_artifacts_examined, findings, verdict.
