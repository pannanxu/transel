//! App Shell：组合 Feature，不承载具体业务逻辑。

mod bootstrap;
mod event_loop;
pub mod request;
pub mod window;

pub use bootstrap::run;
