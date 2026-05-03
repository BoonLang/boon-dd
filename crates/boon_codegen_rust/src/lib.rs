pub fn generated_graph_module(plan: &boon_compiler::CompilePlan) -> String {
    format!("// generated graph for {}\n", plan.source_path)
}
