mod details;
mod downloads;
mod home;
mod navbar;
mod player;
mod poster;
mod search_view;

pub use details::DetailsView;
pub use downloads::DownloadsView;
pub use home::HomeView;
pub use navbar::Navbar;
pub use player::PlayerView;
pub use search_view::SearchView;

const LOGO: &str = include_str!("../../assets/img/vector/logo.svg");
