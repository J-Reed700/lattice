//! The transports every caller uses: a request sent as it is, and one that
//! adds up what the replies passing through it used.

use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;

use super::{send_error, Followup, Sent, Transport, Usage};
use crate::application::ports::llm_port::LLMPort;
use crate::application::services::grounded_generation;
use crate::application::services::model_contract::request::GroundedRequest;
use crate::shared::error::Result;

/// A request sent as it is.
pub struct Plain;

#[async_trait]
impl Transport for Plain {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        let output = grounded_generation::generate(llm, request.clone())
            .await
            .map_err(send_error)?;
        Ok(Sent::of(request, output))
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        self.send(llm, step.0).await
    }
}

/// A transport that adds up the usage of every reply it passes through.
pub struct Metered<'a, T: Transport> {
    inner: &'a T,
    used: Mutex<Usage>,
}

impl<'a, T: Transport> Metered<'a, T> {
    pub fn new(inner: &'a T) -> Self {
        Self {
            inner,
            used: Mutex::new(Usage::default()),
        }
    }

    /// What the replies passed through so far used.
    pub fn used(&self) -> Usage {
        *self.used.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn counted(&self, sent: Sent) -> Sent {
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        used.sends = used.sends.saturating_add(sent.usage.sends);
        used.input = used.input.saturating_add(sent.usage.input);
        used.output = used.output.saturating_add(sent.usage.output);
        sent
    }
}

#[async_trait]
impl<T: Transport> Transport for Metered<'_, T> {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        let sent = self.inner.send(llm, request).await?;
        Ok(self.counted(sent))
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        let sent = self.inner.follow(llm, step).await?;
        Ok(self.counted(sent))
    }

    async fn unanswered(&self, llm: &dyn LLMPort, request: &GroundedRequest) -> Result<()> {
        self.inner.unanswered(llm, request).await
    }
}
