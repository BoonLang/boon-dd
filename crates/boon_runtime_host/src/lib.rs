use boon_dd::SmokeOutput;

#[derive(Default)]
pub struct RuntimeHost;

impl RuntimeHost {
    pub fn run_example(&self, example: &str) -> Option<SmokeOutput> {
        boon_dd::run_named_example_smoke(example)
    }
}
