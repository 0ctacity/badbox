// Syntax fixtures; Badbox does not require type checking.
fn outer(value: String) {
    let label = "🦀 value.clone()";
    // value.clone();
    value.clone();
    value.clone();
    fn inner(value: String) {
        value.clone();
        value.clone();
    }
    let nested = || {
        value.clone();
        value.clone();
    };
}

fn boundary(value: String) { value.clone(); }
fn zero() {}

struct Example;
impl Example {
    fn method(&self) {
        self.clone();
        self.clone();
    }
}
