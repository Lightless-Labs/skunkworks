#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePool {
    allocated: Vec<String>,
}

impl DevicePool {
    pub fn new() -> Self {
        Self { allocated: Vec::new() }
    }

    pub fn allocate(&mut self, resource: &str) {
        self.allocated.push(resource.to_string());
    }

    pub fn release(&mut self, resource: &str) {
        // BUG: missing resources are ignored silently, so callers cannot
        // distinguish a successful release from a failed release.
        if let Some(index) = self.allocated.iter().position(|item| item == resource) {
            self.allocated.remove(index);
        }
    }
}
