//! The engine switch (change `claude-and-opencode-engines`, design D1): one [`Assistant`]
//! that holds every engine and forwards to the one `assistant.engine` names, so the runtime
//! and the gates never know which engine answers and an engine change needs no restart.
//!
//! Each engine is built at start and starts nothing until it is used. A guild's session is
//! detached from the engine it was attached to, whatever is selected by then, so an engine
//! change ends the old engine's sessions on the old engine (and its process stops as after
//! the last leave).

use super::{
    Assistant, AssistantError, AssistantSession, AssistantStatus, Capabilities, PrepareFuture,
};
use crate::audio::AudioPorts;
use crate::settings::{AssistantEngine, Settings};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

pub struct EngineSwitch {
    engines: HashMap<AssistantEngine, Arc<dyn Assistant>>,
    selected: RwLock<AssistantEngine>,
    /// The engine each guild's session belongs to.
    attached: Mutex<HashMap<u64, AssistantEngine>>,
}

impl EngineSwitch {
    /// `engines` holds the engines of this build; `selected` is `assistant.engine` at start.
    pub fn new(
        engines: Vec<(AssistantEngine, Arc<dyn Assistant>)>,
        selected: AssistantEngine,
    ) -> Self {
        Self {
            engines: engines.into_iter().collect(),
            selected: RwLock::new(selected),
            attached: Mutex::new(HashMap::new()),
        }
    }

    pub fn selected(&self) -> AssistantEngine {
        *self.selected.read().expect("engine switch lock")
    }

    fn engine(&self, engine: AssistantEngine) -> Option<&Arc<dyn Assistant>> {
        self.engines.get(&engine)
    }

    fn current(&self) -> Option<&Arc<dyn Assistant>> {
        self.engine(self.selected())
    }

    fn missing(&self) -> String {
        format!(
            "{} is not available in this build",
            self.selected().product_name()
        )
    }
}

impl Assistant for EngineSwitch {
    fn id(&self) -> &'static str {
        self.selected().name()
    }

    fn prepare(&self, guild: u64) -> PrepareFuture<'_> {
        match self.current() {
            Some(engine) => engine.prepare(guild),
            None => {
                let reason = self.missing();
                Box::pin(async move { Err(AssistantError::Unavailable(reason)) })
            }
        }
    }

    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError> {
        let selected = self.selected();
        let engine = self
            .engine(selected)
            .ok_or_else(|| AssistantError::Unavailable(self.missing()))?;
        let session = engine.attach(guild, ports)?;
        self.attached
            .lock()
            .expect("engine switch lock")
            .insert(guild, selected);
        Ok(session)
    }

    fn detach(&self, guild: u64) {
        let owner = self
            .attached
            .lock()
            .expect("engine switch lock")
            .remove(&guild);
        if let Some(engine) = owner.and_then(|engine| self.engine(engine)) {
            engine.detach(guild);
        }
    }

    fn capabilities(&self) -> Capabilities {
        self.current()
            .map(|engine| engine.capabilities())
            .unwrap_or_default()
    }

    fn status(&self) -> AssistantStatus {
        match self.current() {
            Some(engine) => engine.status(),
            None => AssistantStatus {
                engine: self.selected().name().to_owned(),
                summary: self.missing(),
                ..AssistantStatus::default()
            },
        }
    }

    /// Every engine gets the settings; the selected engine follows `assistant.engine`.
    fn configure(&self, settings: &Settings) {
        *self.selected.write().expect("engine switch lock") = settings.assistant.engine;
        for engine in self.engines.values() {
            engine.configure(settings);
        }
    }

    fn stop(&self) {
        for engine in self.engines.values() {
            engine.stop();
        }
    }

    fn use_tts(&self, tts: crate::tts::manager::TtsManager) {
        for engine in self.engines.values() {
            engine.use_tts(tts.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{AnswerSink, StubAssistant};
    use crate::audio::codec::CodecFailures;
    use crate::audio::{frame_channel, output_pair};
    use std::time::Duration;

    fn ports() -> AudioPorts {
        let (_sender, receiver) = frame_channel(4);
        let (output, _source) = output_pair();
        AudioPorts {
            input: receiver,
            output,
            failures: CodecFailures::none(),
            answers: AnswerSink::none(),
            heard: crate::assistant::no_heard(),
        }
    }

    /// Requirement "The engine switches at once": a guild attached to Codex is detached from
    /// Codex after the switch, and the next attach goes to Claude.
    #[test]
    fn sessions_detach_from_their_own_engine_and_attach_to_the_selected_one() {
        let codex = Arc::new(StubAssistant::new(Duration::ZERO));
        let claude = Arc::new(StubAssistant::new(Duration::ZERO));
        let switch = EngineSwitch::new(
            vec![
                (
                    AssistantEngine::Codex,
                    Arc::clone(&codex) as Arc<dyn Assistant>,
                ),
                (
                    AssistantEngine::Claude,
                    Arc::clone(&claude) as Arc<dyn Assistant>,
                ),
            ],
            AssistantEngine::Codex,
        );
        switch.attach(7, ports()).unwrap();
        assert!(codex.session(7).is_some());

        let mut settings = Settings::default();
        settings.assistant.engine = AssistantEngine::Claude;
        switch.configure(&settings);
        assert_eq!(switch.selected(), AssistantEngine::Claude);
        switch.detach(7);
        assert!(codex.session(7).is_none(), "detached from Codex");
        switch.attach(7, ports()).unwrap();
        assert!(claude.session(7).is_some(), "attached to Claude");
        assert!(codex.session(7).is_none());
    }

    #[test]
    fn an_engine_this_build_lacks_refuses_the_join_with_its_name() {
        let switch = EngineSwitch::new(
            vec![(
                AssistantEngine::Codex,
                Arc::new(StubAssistant::default()) as Arc<dyn Assistant>,
            )],
            AssistantEngine::Opencode,
        );
        let error = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(switch.prepare(1))
            .unwrap_err()
            .to_string();
        assert!(error.contains("OpenCode"), "{error}");
        assert!(switch.attach(1, ports()).is_err());
        assert_eq!(switch.status().engine, "opencode");
    }
}
