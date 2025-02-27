use crate::BufferedReader;

use base64::encode;
use nu_engine::CallExt;
use nu_protocol::ast::Call;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::RawStream;

use nu_protocol::{
    Category, Example, PipelineData, ShellError, Signature, Span, SyntaxShape, Value,
};
use reqwest::blocking::Response;

use std::collections::HashMap;
use std::io::BufReader;

use reqwest::StatusCode;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone)]
pub struct SubCommand;

impl Command for SubCommand {
    fn name(&self) -> &str {
        "fetch"
    }

    fn signature(&self) -> Signature {
        Signature::build("fetch")
            .required(
                "URL",
                SyntaxShape::String,
                "the URL to fetch the contents from",
            )
            .named(
                "user",
                SyntaxShape::Any,
                "the username when authenticating",
                Some('u'),
            )
            .named(
                "password",
                SyntaxShape::Any,
                "the password when authenticating",
                Some('p'),
            )
            .named(
                "timeout",
                SyntaxShape::Int,
                "timeout period in seconds",
                Some('t'),
            )
            .named(
                "headers",
                SyntaxShape::Any,
                "custom headers you want to add ",
                Some('H'),
            )
            .switch(
                "raw",
                "fetch contents as text rather than a table",
                Some('r'),
            )
            .filter()
            .category(Category::Network)
    }

    fn usage(&self) -> &str {
        "Fetch the contents from a URL."
    }

    fn extra_usage(&self) -> &str {
        "Performs HTTP GET operation."
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::ShellError> {
        run_fetch(engine_state, stack, call, input)
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                description: "Fetch content from url.com",
                example: "fetch url.com",
                result: None,
            },
            Example {
                description: "Fetch content from url.com, with username and password",
                example: "fetch -u myuser -p mypass url.com",
                result: None,
            },
            Example {
                description: "Fetch content from url.com, with custom header",
                example: "fetch -H [my-header-key my-header-value] url.com",
                result: None,
            },
        ]
    }
}

struct Arguments {
    url: Option<Value>,
    raw: bool,
    user: Option<String>,
    password: Option<String>,
    timeout: Option<Value>,
    headers: Option<Value>,
}

fn run_fetch(
    engine_state: &EngineState,
    stack: &mut Stack,
    call: &Call,
    _input: PipelineData,
) -> Result<nu_protocol::PipelineData, nu_protocol::ShellError> {
    let args = Arguments {
        url: Some(call.req(engine_state, stack, 0)?),
        raw: call.has_flag("raw"),
        user: call.get_flag(engine_state, stack, "user")?,
        password: call.get_flag(engine_state, stack, "password")?,
        timeout: call.get_flag(engine_state, stack, "timeout")?,
        headers: call.get_flag(engine_state, stack, "headers")?,
    };
    helper(engine_state, stack, call, args)
}

// Helper function that actually goes to retrieve the resource from the url given
// The Option<String> return a possible file extension which can be used in AutoConvert commands
fn helper(
    engine_state: &EngineState,
    stack: &mut Stack,
    call: &Call,
    args: Arguments,
) -> std::result::Result<PipelineData, ShellError> {
    let url_value = if let Some(val) = args.url {
        val
    } else {
        return Err(ShellError::UnsupportedInput(
            "Expecting a url as a string but got nothing".to_string(),
            call.head,
        ));
    };

    let span = url_value.span()?;
    let requested_url = url_value.as_string()?;
    let url = match url::Url::parse(&requested_url) {
        Ok(u) => u,
        Err(_e) => {
            return Err(ShellError::UnsupportedInput(
                "Incomplete or incorrect url. Expected a full url, e.g., https://www.example.com"
                    .to_string(),
                span,
            ));
        }
    };
    let user = args.user.clone();
    let password = args.password;
    let timeout = args.timeout;
    let headers = args.headers;
    let raw = args.raw;
    let login = match (user, password) {
        (Some(user), Some(password)) => Some(encode(&format!("{}:{}", user, password))),
        (Some(user), _) => Some(encode(&format!("{}:", user))),
        _ => None,
    };

    let client = http_client();
    let mut request = client.get(url);

    if let Some(timeout) = timeout {
        let val = timeout.as_i64()?;
        if val.is_negative() || val < 1 {
            return Err(ShellError::UnsupportedInput(
                "Timeout value must be an integer and larger than 0".to_string(),
                timeout.span().unwrap_or_else(|_| Span::new(0, 0)),
            ));
        }

        request = request.timeout(Duration::from_secs(val as u64));
    }

    if let Some(login) = login {
        request = request.header("Authorization", format!("Basic {}", login));
    }

    if let Some(headers) = headers {
        let mut custom_headers: HashMap<String, Value> = HashMap::new();

        match &headers {
            Value::List { vals: table, .. } => {
                if table.len() == 1 {
                    // single row([key1 key2]; [val1 val2])
                    match &table[0] {
                        Value::Record { cols, vals, .. } => {
                            for (k, v) in cols.iter().zip(vals.iter()) {
                                custom_headers.insert(k.to_string(), v.clone());
                            }
                        }

                        x => {
                            return Err(ShellError::CantConvert(
                                "string list or single row".into(),
                                x.get_type().to_string(),
                                headers.span().unwrap_or_else(|_| Span::new(0, 0)),
                                None,
                            ));
                        }
                    }
                } else {
                    // primitive values ([key1 val1 key2 val2])
                    for row in table.chunks(2) {
                        if row.len() == 2 {
                            custom_headers.insert(row[0].as_string()?, (&row[1]).clone());
                        }
                    }
                }
            }

            x => {
                return Err(ShellError::CantConvert(
                    "string list or single row".into(),
                    x.get_type().to_string(),
                    headers.span().unwrap_or_else(|_| Span::new(0, 0)),
                    None,
                ));
            }
        };

        for (k, v) in &custom_headers {
            if let Ok(s) = v.as_string() {
                request = request.header(k, s);
            }
        }
    }

    match request.send() {
        Ok(resp) => match resp.headers().get("content-type") {
            Some(content_type) => {
                let content_type = content_type.to_str().map_err(|e| {
                    ShellError::GenericError(
                        e.to_string(),
                        "".to_string(),
                        None,
                        Some("MIME type were invalid".to_string()),
                        Vec::new(),
                    )
                })?;
                let content_type = mime::Mime::from_str(content_type).map_err(|_| {
                    ShellError::GenericError(
                        format!("MIME type unknown: {}", content_type),
                        "".to_string(),
                        None,
                        Some("given unknown MIME type".to_string()),
                        Vec::new(),
                    )
                })?;
                let ext = match (content_type.type_(), content_type.subtype()) {
                    (mime::TEXT, mime::PLAIN) => {
                        let path_extension = url::Url::parse(&requested_url)
                            .map_err(|_| {
                                ShellError::GenericError(
                                    format!("Cannot parse URL: {}", requested_url),
                                    "".to_string(),
                                    None,
                                    Some("cannot parse".to_string()),
                                    Vec::new(),
                                )
                            })?
                            .path_segments()
                            .and_then(|segments| segments.last())
                            .and_then(|name| if name.is_empty() { None } else { Some(name) })
                            .and_then(|name| {
                                PathBuf::from(name)
                                    .extension()
                                    .map(|name| name.to_string_lossy().to_string())
                            });
                        path_extension
                    }
                    _ => Some(content_type.subtype().to_string()),
                };

                let output = response_to_buffer(resp, engine_state, span);

                if raw {
                    return Ok(output);
                }

                if let Some(ext) = ext {
                    match engine_state.find_decl(format!("from {}", ext).as_bytes(), &[]) {
                        Some(converter_id) => engine_state.get_decl(converter_id).run(
                            engine_state,
                            stack,
                            &Call::new(span),
                            output,
                        ),
                        None => Ok(output),
                    }
                } else {
                    Ok(output)
                }
            }
            None => Ok(response_to_buffer(resp, engine_state, span)),
        },
        Err(e) if e.is_timeout() => Err(ShellError::NetworkFailure(
            format!("Request to {} has timed out", requested_url),
            span,
        )),
        Err(e) if e.is_status() => match e.status() {
            Some(err_code) if err_code == StatusCode::NOT_FOUND => Err(ShellError::NetworkFailure(
                format!("Requested file not found (404): {:?}", requested_url),
                span,
            )),
            Some(err_code) if err_code == StatusCode::MOVED_PERMANENTLY => {
                Err(ShellError::NetworkFailure(
                    format!("Resource moved permanently (301): {:?}", requested_url),
                    span,
                ))
            }
            Some(err_code) if err_code == StatusCode::BAD_REQUEST => {
                Err(ShellError::NetworkFailure(
                    format!("Bad request (400) to {:?}", requested_url),
                    span,
                ))
            }
            Some(err_code) if err_code == StatusCode::FORBIDDEN => Err(ShellError::NetworkFailure(
                format!("Access forbidden (403) to {:?}", requested_url),
                span,
            )),
            _ => Err(ShellError::NetworkFailure(
                format!(
                    "Cannot make request to {:?}. Error is {:?}",
                    requested_url,
                    e.to_string()
                ),
                span,
            )),
        },
        Err(e) => Err(ShellError::NetworkFailure(
            format!(
                "Cannot make request to {:?}. Error is {:?}",
                requested_url,
                e.to_string()
            ),
            span,
        )),
    }
}

fn response_to_buffer(
    response: Response,
    engine_state: &EngineState,
    span: Span,
) -> nu_protocol::PipelineData {
    let buffered_input = BufReader::new(response);

    PipelineData::ExternalStream {
        stdout: Some(RawStream::new(
            Box::new(BufferedReader {
                input: buffered_input,
            }),
            engine_state.ctrlc.clone(),
            span,
        )),
        stderr: None,
        exit_code: None,
        span,
        metadata: None,
    }
}

// Only panics if the user agent is invalid but we define it statically so either
// it always or never fails
#[allow(clippy::unwrap_used)]
fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent("nushell")
        .build()
        .unwrap()
}
