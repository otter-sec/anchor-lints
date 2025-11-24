use rustc_span::Span;

#[derive(Debug, Clone)]
pub struct AccountDetails {
    pub span: Span,
    pub account_name: String,
    pub seeds: Vec<String>,
    pub attributes: Vec<String>,
}

#[derive(Debug)]
pub struct DuplicateContextAccounts {
    pub accounts: Vec<AccountDetails>,
}

#[derive(Debug, Clone)]
pub struct AccountConstraint {
    pub mutable: bool,
    pub seeds: Vec<String>,
    pub attributes: Vec<String>,
    pub constraints: Vec<String>,
}

impl AccountConstraint {
    pub fn new() -> Self {
        Self {
            mutable: false,
            seeds: Vec::new(),
            attributes: Vec::new(),
            constraints: Vec::new(),
        }
    }
}
