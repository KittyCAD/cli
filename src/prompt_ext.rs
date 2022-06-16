use anyhow::Result;

pub trait PromptExt {
    fn prompt(base: &str) -> Result<Self>
    where
        Self: Sized;
}
