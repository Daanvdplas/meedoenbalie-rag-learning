//! The application's HTTP limits surround Rig's provider codec.
//!
//! No provider/model loop lives here. Rig creates each request and owns history;
//! this transport preserves our endpoint, byte budgets and safe error boundary.
use super::{
    MAX_HISTORY_BYTES, MAX_RESPONSE_BYTES,
    rig_runtime::{BoundaryError, RunState, check_response},
};
use rig::http_client::{
    Bytes, Error, HttpClientExt, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct BoundedTransport {
    pub client: reqwest::Client,
    pub endpoint: reqwest::Url,
    pub state: Arc<Mutex<RunState>>,
}
impl BoundedTransport {
    fn failure(&self, error: &'static str) -> Error {
        if let Ok(mut state) = self.state.lock() {
            state.error = Some(error);
        }
        Error::instance(BoundaryError(error))
    }

    async fn request(&self, request: Request<Bytes>) -> Result<Vec<u8>, Error> {
        if request.method() != reqwest::Method::POST || request.uri() != self.endpoint.as_str() {
            return Err(self.failure("runtime: API-adres is niet toegestaan."));
        }
        if request.body().len() > MAX_HISTORY_BYTES {
            return Err(self.failure("output: De modelcontext is te groot."));
        }
        {
            let mut state = self.state.lock().map_err(|_| {
                Error::instance(BoundaryError("runtime: De uitvoering is niet beschikbaar."))
            })?;
            // Counts physical HTTP attempts, independently of Rig's turn budget.
            if state.requests >= 5 || state.error.is_some() {
                drop(state);
                return Err(self.failure("tool: Het maximum aantal modelverzoeken is bereikt."));
            }
            state.requests += 1;
        }
        let (parts, body) = request.into_parts();
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(|_| {
                self.failure("runtime: De antwoordservice is tijdelijk niet bereikbaar.")
            })?;
        if !response.status().is_success() {
            return Err(self.failure(match response.status().as_u16() {
                401 | 403 => "runtime: De antwoordservice is niet juist ingesteld.",
                402 | 429 => {
                    "runtime: De antwoordservice heeft tijdelijk geen capaciteit of tegoed."
                }
                _ => "runtime: De antwoordservice is tijdelijk niet beschikbaar.",
            }));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| self.failure("runtime: Het antwoord kon niet worden ontvangen."))?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(self.failure("output: Het antwoord bevat te veel gegevens."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| {
            self.failure("output: De antwoordservice heeft ongeldige uitvoer geleverd.")
        })?;
        let checked = {
            let mut state = self.state.lock().map_err(|_| {
                Error::instance(BoundaryError("runtime: De uitvoering is niet beschikbaar."))
            })?;
            check_response(&raw, &mut state)
        };
        checked.map_err(|error| self.failure(error.0))?;
        Ok(bytes)
    }
}

impl HttpClientExt for BoundedTransport {
    fn send<T, U>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        let transport = self.clone();
        let request = request.map(Into::into);
        async move {
            let bytes = transport.request(request).await?;
            let body: LazyBody<U> = Box::pin(async move { Ok(U::from(Bytes::from(bytes))) });
            Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(body)
                .map_err(Error::Protocol)
        }
    }
    fn send_multipart<U>(
        &self,
        _: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        let error = self.failure("tool: Onverwacht API-gebruik is niet toegestaan.");
        async move { Err(error) }
    }
    fn send_streaming<T>(
        &self,
        _: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse, Error>> + Send
    where
        T: Into<Bytes> + Send,
    {
        let error = self.failure("tool: Onverwacht API-gebruik is niet toegestaan.");
        async move { Err(error) }
    }
}
