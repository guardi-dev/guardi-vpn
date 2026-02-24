use color_eyre::owo_colors::Style;
use ratatui::{
    DefaultTerminal, Frame, buffer::Buffer, crossterm::event::{Event, KeyCode}, layout::{Constraint, Layout, Rect}, style::{Color, Stylize, palette::tailwind}, symbols, text::{Line}, widgets::{Block, Padding, Paragraph, Tabs, Widget}
};
use crossterm::event::{EventStream};
use crate::network::broadcast::{P2PBroadcast, EngineEvent};
use futures_util::StreamExt; // Важно для метода .next()
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};

#[derive(Default, Clone, Copy, Display, FromRepr, EnumIter, EnumCount)]
enum SelectedTab {
    #[default]
    #[strum(to_string = "Chat")]
    Chat,
    #[strum(to_string = "Logs")]        
    Logs,
}

/// App holds the state of the application
pub struct App {
    /// Current value of the input box
    input: String,
    /// Position of cursor in the editor area.
    character_index: usize,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Vec<String>,

    selected_tab: SelectedTab
}


enum InputMode {
    Normal,
    Editing,
}

impl App {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: Vec::new(),
            character_index: 0,
            selected_tab: SelectedTab::Chat
        }
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can be contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    fn submit_message(&mut self) {
        // self.messages.push(self.input.clone());
        self.input.clear();
        self.reset_cursor();
    }

	pub async fn default (broadcast: &P2PBroadcast) {
		color_eyre::install().unwrap();
		let terminal = ratatui::init();
		let _ = App::new().run(terminal, broadcast).await;
		App::restore();
	}

	pub async fn default_broadcast () {
		color_eyre::install().unwrap();
		let terminal = ratatui::init();
		let broadcast = P2PBroadcast::new();
		let _ = App::new().run(terminal, &broadcast).await;
		App::restore();
	}

	pub fn restore () {
		ratatui::restore();
	}

    pub fn next_tab(&mut self) {
        self.selected_tab = self.selected_tab.next();
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal, broadcast: &P2PBroadcast) -> Result<(), anyhow::Error> {
		let mut tx = broadcast.subscribe();
		let mut reader = EventStream::new();

        loop {
			terminal.draw(|frame| self.draw(frame))?;

			tokio::select! {
				biased;

				key = reader.next() => {
                    match key {
                        Some(Ok(Event::Key(key))) => {
                            match key.code {
                                KeyCode::Char('q') => {
                                    return Ok(());
                                }
                                KeyCode::Enter => {
                                    // send message to p2p subscribers
                                    broadcast.send_message(self.input.clone());
                                    self.submit_message();
                                },
                                KeyCode::Tab => self.next_tab(),
                                KeyCode::Char(to_insert) => self.enter_char(to_insert),
                                KeyCode::Backspace => self.delete_char(),
                                KeyCode::Left => self.move_cursor_left(),
                                KeyCode::Right => self.move_cursor_right(),
                                KeyCode::Esc => self.input_mode = InputMode::Normal,
                                _ => {}
                            }
                        },
                        _ => {}
                    }
				}
				
				event = tx.recv() => {
					match event {
						Ok(EngineEvent::Chat(msg)) => {
							let user_message = format!("[{}] {}", msg.sender, msg.content);
							self.messages.push(user_message);
						}
						_ => {}
					}
				}
			}
        }
    }

    fn draw(&self, frame: &mut Frame) {
        use Constraint::{Length, Min};

        let area = frame.area();
        let buf = frame.buffer_mut();

        // === LAYOUT ===
        let rows = Layout::vertical([Length(1), Min(0), Length(1)]);
        let header_row = Layout::horizontal([Min(0), Length(20)]);
        let [header_area, inner_area, footer_area] = rows.areas(area);
        let [tabs_area, title_area] = header_row.areas(header_area);

        render_title(title_area, buf);

        // === RENDER TABS === 
        let titles = SelectedTab::iter().map(|t| t.title());
        let highlight_style = (Color::default(), self.selected_tab.palette().c700);
        let selected_tab_index = self.selected_tab as usize;
        Tabs::new(titles)
            .highlight_style(highlight_style)
            .select(selected_tab_index)
            .padding("", "")
            .divider(" ")
            .bold()
            .render(tabs_area, buf);

        // == TAB BODY ===
        match self.selected_tab {
            SelectedTab::Chat => {
                Paragraph::new(format!(" {} ", self.input.as_str()))
                    .block(Block::bordered().title("Input"))
                    .render(inner_area, buf);
            },
            SelectedTab::Logs => {

            }
        }
        
        Paragraph::new("")
            .block(Block::bordered()
                .border_set(symbols::border::PROPORTIONAL_TALL)
                .padding(Padding::horizontal(1))
                .border_style(self.selected_tab.palette().c700))
            .render(inner_area, buf);

        // === FOOTER ===
        render_footer(footer_area, buf);    
    }
}

impl SelectedTab {
    /// Get the next tab, if there is no next tab return the current tab.
    fn next(self) -> Self {
        let current_index = self as usize;
        let mut next_index = current_index.saturating_add(1);
        if current_index == SelectedTab::COUNT - 1 {
            next_index = 0;
        }
        Self::from_repr(next_index).unwrap_or(self)
    }

    fn title(self) -> Line<'static> {
        format!("  {self}  ")
            .fg(tailwind::SLATE.c200)
            .bg(self.palette().c900)
            .into()
    }

    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::Chat => tailwind::BLUE,
            Self::Logs => tailwind::EMERALD,
        }
    }
}

fn render_title(area: Rect, buf: &mut Buffer) {
    "Guardi VPN".bold().render(area, buf);
}

fn render_footer(area: Rect, buf: &mut Buffer) {
    Line::raw("Press 'Tab' change to next tab | Press 'Q' to quit")
        .centered()
        .render(area, buf);
}