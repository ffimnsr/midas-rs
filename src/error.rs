use std::error::Error as StdError;

pub type GenericError = Box<dyn StdError + Send + Sync>;
