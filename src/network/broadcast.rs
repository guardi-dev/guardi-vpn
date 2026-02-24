use tokio::sync::broadcast;

// 1. Декларируем конкретные типы данных (наши "события")
#[derive(Clone, Debug)]
pub struct UserMessage {
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct StatsMessage {
	pub room_count: u32,
	pub kademlia_count: u32,
	pub gosibsub_count: u32,
	pub active_ralays_count: u32
}

// 2. Объединяем их в один "Union" (EngineEvent)
#[derive(Clone, Debug)]
pub enum EngineEvent {
    User(UserMessage),
    Chat(ChatMessage),      // Теперь это четкая ссылка на структуру
	Stats(StatsMessage)
}


pub struct P2PBroadcast {
    // Передатчик событий
    tx: broadcast::Sender<EngineEvent>,
}

impl P2PBroadcast {
    pub fn new() -> Self {
        // Создаем канал с буфером (например, 32 сообщения)
        let (tx, _) = broadcast::channel(32);
        Self { tx }
    }

    // Метод для подписки: возвращает "приемник"
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.tx.subscribe()
    }

    // === Subscriber MESSAGES ===
    pub fn send_message (&self, message: String) {
        let event = EngineEvent::User(UserMessage {
            content: message.clone()
        });
        let _ = self.tx.send(event);
    }

	// === P2P MESSAGES ===
	pub fn on_stats (&self, message: StatsMessage) {
		let event = EngineEvent::Stats(message);
        let _ = self.tx.send(event);
	}

    pub fn on_network_room_message(&self, message: ChatMessage) {
        let event = EngineEvent::Chat(message);
        let _ = self.tx.send(event);
    }
}
