use crate::bridge::PlayerStatus;

pub struct App {
    pub should_quit: bool,
    pub player: PlayerStatus,
    pub active_panel: Panel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    NowPlaying,
    Library,
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            player: PlayerStatus::default(),
            active_panel: Panel::Library,
        }
    }
}

impl App {
    pub fn update_player(&mut self, status: PlayerStatus) {
        self.player = status;
    }
}
