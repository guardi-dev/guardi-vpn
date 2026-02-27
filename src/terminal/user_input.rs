use ratatui::{
    DefaultTerminal, Frame, buffer::Buffer, crossterm::event::{Event, KeyCode}, layout::{Alignment, Constraint::{self, Percentage}, Layout, Margin, Rect}, style::{Color, Stylize, palette::tailwind}, symbols, text::{Line, Span}, widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Tabs, Widget}
};
use crossterm::event::{EventStream, KeyEventKind, KeyModifiers};
use uuid::Uuid;
use crate::network::broadcast::{ChatMessage, EngineEvent, P2PBroadcast, StatsMessage};
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
    input: String,

    character_index: usize,

    input_mode: InputMode,
    
    messages: Vec<ChatMessage>,

    selected_tab: SelectedTab,

    logs: Vec<String>,

    list_state: ListState,
    
    scroll_state: ScrollbarState,
    
    auto_scroll: bool,

    stats: StatsMessage
}

const MAX_LOGS: u32 = 1000;

enum InputMode {
    Normal,
}

impl App {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: Vec::new(),
            logs: Vec::new(),
            character_index: 0,
            selected_tab: SelectedTab::Chat,
            list_state: ListState::default(),
            scroll_state: ScrollbarState::default(),
            auto_scroll: true,
            stats: StatsMessage { 
                room_count: 0, 
                kademlia_count: 0, 
                gosibsub_count: 0, 
                active_ralays_count: 0 
            }
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
        self.messages.push(ChatMessage { id: Uuid::new_v4().to_string(), sender: "Me".to_string(), content: self.input.clone() });
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

    fn scroll_down (&mut self) {
        self.auto_scroll = true;
        self.scroll_state.next();
        self.list_state.scroll_down_by(1);
    }

    fn scroll_up (&mut self) {
        self.auto_scroll = false;
        self.scroll_state.prev();
        self.list_state.scroll_up_by(1);
    }
    
    fn scroll_last (&mut self) {
        self.scroll_state.last();
        self.list_state.select_last();
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
                            match (key.modifiers, key.code) {
                                (KeyModifiers::CONTROL, KeyCode::Char('q')) => { return Ok(()) }
                                (_, KeyCode::Esc) => return Ok(()),
                                _ => {}
                            }
                            if key.kind == KeyEventKind::Press { match key.code {
                                KeyCode::Down => {
                                    self.scroll_down();
                                }
                                KeyCode::Up => {
                                    self.scroll_up();
                                }
                                KeyCode::Enter => {
                                    if self.stats.room_count == 0 {
                                        continue;
                                    }
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
                            }}
                        },
                        _ => {}
                    }
				}
				
				Ok(event) = tx.recv() => {
					match event {
						EngineEvent::Chat(msg) => {
							self.messages.push(msg);
						}
                        EngineEvent::Log(msg) => {
                            self.logs.push(msg.content);
                            if self.logs.len() > MAX_LOGS.try_into().unwrap() {
                                self.logs.remove(0);
                            }
                        }
                        EngineEvent::Stats(msg) => {
                            self.stats = msg;
                        }
						_ => {}
					}
				}
			}
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        use Constraint::{Length, Min};

        let area = frame.area();
        let buf = frame.buffer_mut();

        // === LAYOUT ===
        let rows = Layout::vertical([Length(1), Min(0), Length(1)]);
        let header_row = Layout::horizontal([Min(0), Min(0)]);
        let [header_area, inner_area, footer_area] = rows.areas(area);
        let [tabs_area, title_area] = header_row.areas(header_area);

        // === TITLE ===
        Line::raw(format!(
            "Guardi VPN 📚:{} 🌐:{} 📡:{} 💬:{}", 
            self.stats.kademlia_count, 
            self.stats.gosibsub_count,
            self.stats.active_ralays_count,
            self.stats.room_count
        ))
            .alignment(Alignment::Right)
            .bold()
            .render(title_area, buf);

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
                let chat_layout = Layout::vertical([Min(3), Percentage(100)]);
                let [ input_area, message_area ] = chat_layout.areas(inner_area);
                let horizontal_padding = 2;
                let placeholder= if self.stats.room_count > 0 { "Write your message here" } else { "Discovery peers... Please wait." };
                // === INPUT ===
                Paragraph::new(format!("{}", if self.input.len() > 0 { self.input.as_str() } else { placeholder }))
                    .fg(if self.input.len() > 0 { Color::White } else { Color::DarkGray })
                    .block(Block::new()
                        .padding(Padding::horizontal(horizontal_padding.clone()))
                        .title("Input"))
                    .render(input_area, buf);

                // === MESSAGES ===
                let messages: Vec<ListItem> = self.messages
                    .iter()
                    .map(|l| {
                        let max = 8;
                        let title_text = format!(
                            "{}: ", 
                            if l.sender.len() > max {
                                truncate_start(&l.sender, max)
                            } else { 
                                l.sender.to_string() 
                            }
                        );
                        let title = Span::from(title_text)
                            .fg(Color::DarkGray);
                        let content = Span::from(l.content.to_string());
                        let row = Line::from(vec![title, content]);
                        ListItem::new(row)
                    })
                    .collect();

                let scroll = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓"));

                self.scroll_state = self.scroll_state.content_length(messages.clone().len());
                if self.auto_scroll {
                    self.scroll_last();
                }

                let list = List::new(messages.clone())
                    .block(Block::new()
                    .padding(Padding::horizontal(horizontal_padding.clone())));
                let scroll_render = message_area.inner(Margin {
                    vertical: 0,
                    horizontal: 0
                });

                StatefulWidget::render(list, scroll_render, buf, &mut self.list_state);
                StatefulWidget::render(scroll, scroll_render, buf, &mut self.scroll_state);
            },
            SelectedTab::Logs => {
                let logs: Vec<ListItem> = self.logs
                    .iter()
                    .map(|l| {
                        let content = Line::from(Span::raw(format!("{l}")));
                        ListItem::new(content)
                    })
                    .collect();

                let scroll = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓"));

                self.scroll_state = self.scroll_state.content_length(logs.clone().len());
                if self.auto_scroll {
                    self.scroll_last();
                }

                let list = List::new(logs.clone())
                    .block(Block::bordered());

                let scroll_render = inner_area.inner(Margin {
                    vertical: 0,
                    horizontal: 1
                });

                StatefulWidget::render(list, scroll_render, buf, &mut self.list_state);
                StatefulWidget::render(scroll, scroll_render, buf, &mut self.scroll_state);
            }
        }
        
        // === INNER BORDER ===
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

fn render_footer(area: Rect, buf: &mut Buffer) {
    Line::raw("Press 'Tab' change to next tab | Press 'Ctrl+Q' or 'Esc' to quit | ▲ ▼ to scroll")
        .centered()
        .render(area, buf);
}

fn truncate_start(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    // Пропускаем лишние символы в начале
    let truncated: String = s.chars().skip(count - max).collect();
    format!("...{}", truncated)
}