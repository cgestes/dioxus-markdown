//! Topics in a SilverBullet space that a ZeroClaw agent maintains.
//!
//! The space is the source of truth. A top-level folder is a topic (`car/`,
//! `wedding/`) and its sub-folders are sub-topics. Pages in `people/`, and any
//! page a channel routes to directly, are topics of their own
//! (`people/nikki-sample`). The space README maps Slack and Telegram chats to
//! folders, `reminders.md` holds every deadline, and `<folder>/questions.md`
//! holds open questions. This crate only parses; `osom-hub` does the I/O.
//!
//! `space` has the topics, `routes` the channels that feed them, `planning`
//! reminders, key dates and questions, `chats` the agent's conversations,
//! `page` what is read from a page, and `calendar` how a date is written.
pub mod calendar;
mod chats;
mod page;
mod planning;
mod routes;
mod space;
#[cfg(test)]
mod tests;

pub use chats::*;
pub use page::*;
pub use planning::*;
pub use routes::*;
pub use space::*;
