mod details;
mod downloads;
mod helpers;
mod home;
mod navbar;
mod player;
mod search_view;

pub use details::DetailsView;
pub use downloads::DownloadsView;
pub use home::HomeView;
pub use navbar::Navbar;
pub use player::PlayerView;
pub use search_view::SearchView;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Home,
    Search,
    Details,
    Player,
    Downloads,
}
