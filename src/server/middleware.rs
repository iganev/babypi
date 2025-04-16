use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorUnauthorized,
    Error,
};
use futures_util::future::{ready, LocalBoxFuture, Ready};
use std::sync::Arc;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// Authentication provider trait
trait AuthProvider: Send + Sync {
    fn validate_basic(&self, username: &str, password: &str) -> bool;
    fn validate_bearer(&self, token: &str) -> bool;
}

// Concrete implementation of the auth provider
struct ConfigAuthProvider {
    basic_credentials: Vec<(String, String)>,
    bearer_tokens: Vec<String>,
}

// Auth provider implementation
impl AuthProvider for ConfigAuthProvider {
    fn validate_basic(&self, username: &str, password: &str) -> bool {
        self.basic_credentials
            .iter()
            .any(|(u, p)| u == username && p == password)
    }

    fn validate_bearer(&self, token: &str) -> bool {
        self.bearer_tokens.contains(&token.to_string())
    }
}

// Auth provider factory
impl ConfigAuthProvider {
    fn new() -> Self {
        // Default empty implementation
        Self {
            basic_credentials: Vec::new(),
            bearer_tokens: Vec::new(),
        }
    }

    // Add credentials programmatically
    fn add_basic_auth(&mut self, username: String, password: String) {
        self.basic_credentials.push((username, password));
    }

    fn add_bearer_token(&mut self, token: String) {
        self.bearer_tokens.push(token);
    }
}

// Combined middleware for authentication and HLS headers
struct AuthAndHlsHeadersMiddleware {
    auth_provider: Arc<dyn AuthProvider>,
}

impl AuthAndHlsHeadersMiddleware {
    fn new(auth_provider: Arc<dyn AuthProvider>) -> Self {
        Self { auth_provider }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AuthAndHlsHeadersMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = AuthAndHlsHeadersMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthAndHlsHeadersMiddlewareService {
            service,
            auth_provider: self.auth_provider.clone(),
        }))
    }
}

struct AuthAndHlsHeadersMiddlewareService<S> {
    service: S,
    auth_provider: Arc<dyn AuthProvider>,
}

impl<S, B> Service<ServiceRequest> for AuthAndHlsHeadersMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // If auth fails, return early with error
        if let Err(e) = self.authenticate(&req) {
            // let (request, _pl) = req.into_parts();

            // let response = HttpResponse::Unauthorized()
            //     .insert_header((
            //         actix_web::http::header::WWW_AUTHENTICATE,
            //         "Basic realm=\"BabyPi\", Bearer",
            //     ))
            //     .finish();

            // return Box::pin(futures_util::future::ok(ServiceResponse::new(
            //     request, response,
            // )));

            return Box::pin(futures_util::future::err(e));
        }

        let fut = self.service.call(req);

        Box::pin(async move {
            let mut res = fut.await?;

            let path = res.request().path();

            if path.ends_with(".m3u8") {
                res.headers_mut().insert(
                    actix_web::http::header::CONTENT_TYPE,
                    actix_web::http::header::HeaderValue::from_static(
                        "application/vnd.apple.mpegurl",
                    ),
                );
                res.headers_mut().insert(
                    actix_web::http::header::CACHE_CONTROL,
                    actix_web::http::header::HeaderValue::from_static("no-cache"),
                );
            } else if path.ends_with(".ts") {
                res.headers_mut().insert(
                    actix_web::http::header::CONTENT_TYPE,
                    actix_web::http::header::HeaderValue::from_static("video/MP2T"),
                );
                res.headers_mut().insert(
                    actix_web::http::header::CACHE_CONTROL,
                    actix_web::http::header::HeaderValue::from_static("no-cache"),
                );
                res.headers_mut().insert(
                    actix_web::http::header::ACCEPT_RANGES,
                    actix_web::http::header::HeaderValue::from_static("bytes"),
                );
            }

            // Common headers for all HLS content
            res.headers_mut().insert(
                actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                actix_web::http::header::HeaderValue::from_static("*"),
            );
            res.headers_mut().insert(
                actix_web::http::header::ACCESS_CONTROL_ALLOW_METHODS,
                actix_web::http::header::HeaderValue::from_static("GET, POST, HEAD, OPTIONS"),
            );
            res.headers_mut().insert(
                actix_web::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
                actix_web::http::header::HeaderValue::from_static("Range, Authorization"),
            );

            Ok(res)
        })
    }
}

// Authentication helper function moved into the middleware service
impl<S, B> AuthAndHlsHeadersMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    fn authenticate(&self, req: &ServiceRequest) -> Result<(), Error> {
        // Skip authentication for OPTIONS requests (CORS preflight)
        if req.method() == actix_web::http::Method::OPTIONS {
            return Ok(());
        }

        // Get authorization header
        if let Some(auth_header) = req.headers().get(actix_web::http::header::AUTHORIZATION) {
            if let Ok(auth_str) = auth_header.to_str() {
                // Check if it's Basic auth
                if auth_str.starts_with("Basic ") {
                    let credentials = auth_str.trim_start_matches("Basic ");
                    if let Ok(decoded) = base64::decode(credentials) {
                        if let Ok(auth_string) = String::from_utf8(decoded) {
                            let parts: Vec<&str> = auth_string.splitn(2, ':').collect();
                            if parts.len() == 2 {
                                let username = parts[0];
                                let password = parts[1];
                                if self.auth_provider.validate_basic(username, password) {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                // Check if it's Bearer auth
                else if auth_str.starts_with("Bearer ") {
                    let token = auth_str.trim_start_matches("Bearer ");
                    if self.auth_provider.validate_bearer(token) {
                        return Ok(());
                    }
                }
            }
        }

        // If we get here, authentication failed
        // let mut response = req.response.header(
        //     actix_web::http::header::WWW_AUTHENTICATE,
        //     "Basic realm=\"BabyPi\", Bearer",
        // );

        Err(ErrorUnauthorized("Unauthorized"))
    }
}

// The middleware struct that will inject headers on 401 responses
pub struct UnauthorizedHeaderInjector {
    header_name: String,
    header_value: String,
}

impl UnauthorizedHeaderInjector {
    pub fn new(header_name: impl Into<String>, header_value: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            header_value: header_value.into(),
        }
    }
}

// Middleware factory is `Transform` trait
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for UnauthorizedHeaderInjector
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = UnauthorizedHeaderInjectorMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        futures_util::future::ok(UnauthorizedHeaderInjectorMiddleware {
            service,
            header_name: self.header_name.clone(),
            header_value: self.header_value.clone(),
        })
    }
}

pub struct UnauthorizedHeaderInjectorMiddleware<S> {
    service: S,
    header_name: String,
    header_value: String,
}

impl<S, B> Service<ServiceRequest> for UnauthorizedHeaderInjectorMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let header_name = self.header_name.clone();
        let header_value = self.header_value.clone();

        let fut = self.service.call(req);

        Box::pin(async move {
            let mut res = fut.await?;

            // Check if the response status is 401 Unauthorized
            if res.status().as_u16() == 401 {
                // Insert the header
                res.headers_mut()
                    .insert(header_name.parse().unwrap(), header_value.parse().unwrap());
            }

            Ok(res)
        })
    }
}
