//! The MCP tool surface: every usecode-agent-api endpoint the bot exposes, plus
//! the local `deploy/compose.yml` lifecycle.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::client::{ApiResult, Client};
use crate::compose;
use crate::config::Settings;

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ApiKeyArgs {
    /// Api key to authenticate with. Falls back to USECODE_MCP_API_KEY.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PhoneArgs {
    /// Phone number in E.164 format, e.g. +14155552671.
    pub phone: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VerifyOtpArgs {
    /// Phone number the code was sent to, in E.164 format.
    pub phone: String,
    /// The one-time code received by that phone number.
    pub code: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateApiKeyArgs {
    /// What the key is for, e.g. "laptop" or "ci".
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RevokeApiKeyArgs {
    /// Id of the api key to revoke, as listed by list_api_keys.
    pub key_id: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModelStartArgs {
    /// Container image to run llama-server from.
    #[serde(default)]
    pub image: Option<String>,
    /// Hugging Face repo of the model, e.g. "ggml-org/Qwen3-0.6B-GGUF:Q4_0".
    #[serde(default)]
    pub hf_repo: Option<String>,
    /// Device to run on, e.g. "Vulkan0".
    #[serde(default)]
    pub device: Option<String>,
    /// Number of layers to offload to the GPU.
    #[serde(default)]
    pub ngl: Option<i64>,
    /// Model alias the server answers to, e.g. "local-model".
    #[serde(default)]
    pub alias: Option<String>,
    /// Context size in tokens.
    #[serde(default)]
    pub ctx_size: Option<i64>,
    /// Address to bind on.
    #[serde(default)]
    pub host: Option<String>,
    /// Port to listen on.
    #[serde(default)]
    pub port: Option<i64>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetProviderCredentialsArgs {
    /// Cloud provider, e.g. "hetzner" or "digitalocean".
    pub provider: String,
    /// Credentials object whose shape depends on the provider.
    pub credentials: Value,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProviderArgs {
    /// Cloud provider, e.g. "hetzner" or "digitalocean".
    pub provider: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServerIdArgs {
    /// The usecode agent server id.
    pub server_id: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

fn default_image() -> String {
    "ubuntu-24.04".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateServerArgs {
    /// Name for the new server.
    pub name: String,
    /// usecode agent's own series-city type string, e.g. "x1-fsn1" or "y1-nyc3".
    #[serde(rename = "type")]
    pub server_type: String,
    /// OS image slug, e.g. "ubuntu-24.04".
    #[serde(default = "default_image")]
    pub image: String,
    /// Ids or names of ssh keys to install on the server.
    #[serde(default)]
    pub ssh_keys: Option<Vec<String>>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskIdArgs {
    /// Id of the background task.
    pub task_id: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CatalogArgs {
    /// Only entries from this provider ("hetzner"/"digitalocean").
    #[serde(default)]
    pub provider: Option<String>,
    /// Only entries of this kind ("location"/"server_type"/"image").
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// An API answer, or the `{"error", "status_code"}` shape every tool reports
/// instead of failing the call outright.
fn answer(result: ApiResult<Value>) -> Json<Value> {
    Json(match result {
        Ok(value) => value,
        Err(error) => error.to_value(),
    })
}

/// The same, for endpoints that answer with no body: a fixed status line.
fn acknowledge(result: ApiResult<()>, status: &str) -> Json<Value> {
    Json(match result {
        Ok(()) => json!({"status": status}),
        Err(error) => error.to_value(),
    })
}

fn status_with(status: &str, fields: Map<String, Value>) -> Value {
    let mut map = Map::new();
    map.insert("status".to_string(), json!(status));
    map.extend(fields);
    Value::Object(map)
}

#[derive(Clone)]
pub struct UsecodeServer {
    settings: Arc<Settings>,
    client: Client,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl UsecodeServer {
    pub fn new(settings: Arc<Settings>) -> crate::error::Result<Self> {
        Ok(Self {
            client: Client::new(settings.clone())?,
            settings,
            tool_router: Self::tool_router(),
        })
    }

    /// Request a one-time login code for a usecode agent phone number (E.164 format, e.g. +14155552671).
    #[tool]
    async fn request_otp(&self, args: Parameters<PhoneArgs>) -> Json<Value> {
        answer(self.client.request_otp(&args.0.phone).await)
    }

    /// Verify a usecode agent OTP code and return an api_key for authenticated calls.
    #[tool]
    async fn verify_otp(&self, args: Parameters<VerifyOtpArgs>) -> Json<Value> {
        let args = args.0;
        answer(self.client.verify_otp(&args.phone, &args.code).await)
    }

    /// Get the phone number tied to a usecode agent api_key. Falls back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn me(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.me(args.0.api_key).await)
    }

    /// Revoke a usecode agent api_key, logging that client out.
    #[tool]
    async fn logout(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        acknowledge(self.client.logout(args.0.api_key).await, "logged out")
    }

    /// Generate a new usecode agent API key for the caller's account, authenticated
    /// with an existing api_key (or the configured USECODE_MCP_API_KEY). Use
    /// `label` to note what the key is for (e.g. "laptop", "ci").
    #[tool]
    async fn create_api_key(&self, args: Parameters<CreateApiKeyArgs>) -> Json<Value> {
        let args = args.0;
        answer(self.client.create_api_key(&args.label, args.api_key).await)
    }

    /// List the caller's usecode agent API keys (id, label, timestamps — never the
    /// key value itself, which is only shown once at creation).
    #[tool]
    async fn list_api_keys(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.list_api_keys(args.0.api_key).await)
    }

    /// Revoke one of the caller's usecode agent API keys by id.
    #[tool]
    async fn revoke_api_key(&self, args: Parameters<RevokeApiKeyArgs>) -> Json<Value> {
        let args = args.0;
        acknowledge(
            self.client.revoke_api_key(&args.key_id, args.api_key).await,
            "revoked",
        )
    }

    /// Check whether usecode agent is reachable. Reports every configured endpoint
    /// (the Caddy load balancers requests are spread over round-robin), not
    /// just the one the next request would land on, so a single dead load
    /// balancer shows up instead of being silently failed over. Each reachable
    /// entry names the API node that answered it.
    #[tool]
    async fn health(&self) -> Json<Value> {
        let endpoints = self.client.health_all().await;
        let reachable = endpoints
            .iter()
            .filter(|entry| entry.get("reachable") == Some(&Value::Bool(true)))
            .count();
        Json(json!({
            "status": if reachable > 0 { "ok" } else { "unreachable" },
            "reachable": reachable,
            "endpoints": endpoints,
        }))
    }

    /// List the configurable fields for kick-starting the AI model container
    /// (llama-server), each with its default value and, where applicable, its
    /// allowed options. Falls back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn model_options(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.model_options(args.0.api_key).await)
    }

    /// Check whether the AI model container (llama-server) is currently running
    /// on the usecode-agent-api host. Falls back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn model_status(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.model_status(args.0.api_key).await)
    }

    /// Kick-start the AI model container (llama-server) on the usecode-agent-api host.
    /// Defaults to `ggml-org/Qwen3-0.6B-GGUF:Q4_0` on device `Vulkan0` with a
    /// 32768-token context, alias `local-model`, on `127.0.0.1:8080` — call
    /// model_options for the full default/options list. Omit any field to
    /// keep its default; pass a value to override just that field. Falls back to
    /// the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn model_start(&self, args: Parameters<ModelStartArgs>) -> Json<Value> {
        let args = args.0;
        let mut overrides = Map::new();
        let mut set = |name: &str, value: Option<Value>| {
            if let Some(value) = value {
                overrides.insert(name.to_string(), value);
            }
        };
        set("image", args.image.map(Value::from));
        set("hf_repo", args.hf_repo.map(Value::from));
        set("device", args.device.map(Value::from));
        set("ngl", args.ngl.map(Value::from));
        set("alias", args.alias.map(Value::from));
        set("ctx_size", args.ctx_size.map(Value::from));
        set("host", args.host.map(Value::from));
        set("port", args.port.map(Value::from));
        answer(
            self.client
                .model_start(Value::Object(overrides), args.api_key)
                .await,
        )
    }

    /// Stop the running AI model container (llama-server) on the usecode-agent-api host.
    /// Falls back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn model_stop(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        acknowledge(self.client.model_stop(args.0.api_key).await, "stopped")
    }

    /// Store the caller's credentials for a cloud provider, encrypted at
    /// rest, so servers of that provider's type series can be created/synced.
    /// `credentials` is a JSON object whose shape depends on `provider`:
    /// - "hetzner": {"apiKey": "<hetzner cloud api token>"}
    /// - "digitalocean": {"apiKey": "<digitalocean api token>"}
    /// Other providers may require different fields (e.g. clientId/
    /// clientSecret) — check that provider's docs. Falls back to the
    /// configured USECODE_MCP_API_KEY.
    #[tool]
    async fn set_provider_credentials(
        &self,
        args: Parameters<SetProviderCredentialsArgs>,
    ) -> Json<Value> {
        let args = args.0;
        answer(
            self.client
                .set_provider_credentials(&args.provider, args.credentials, args.api_key)
                .await,
        )
    }

    /// Check whether the caller has credentials configured for one cloud
    /// provider ("hetzner" or "digitalocean"). Falls back to the configured
    /// USECODE_MCP_API_KEY.
    #[tool]
    async fn provider_credentials_status(&self, args: Parameters<ProviderArgs>) -> Json<Value> {
        let args = args.0;
        answer(
            self.client
                .provider_credentials_status(&args.provider, args.api_key)
                .await,
        )
    }

    /// Remove the caller's stored credentials for a cloud provider
    /// ("hetzner" or "digitalocean"). Falls back to the configured
    /// USECODE_MCP_API_KEY.
    #[tool]
    async fn delete_provider_credentials(&self, args: Parameters<ProviderArgs>) -> Json<Value> {
        let args = args.0;
        acknowledge(
            self.client
                .delete_provider_credentials(&args.provider, args.api_key)
                .await,
            "deleted",
        )
    }

    /// List every supported cloud provider ("hetzner", "digitalocean") and
    /// whether the caller has credentials configured for it. Falls back to the
    /// configured USECODE_MCP_API_KEY.
    #[tool]
    async fn list_provider_credentials(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.list_provider_credentials(args.0.api_key).await)
    }

    /// List the caller's servers, in usecode agent's own terms — id, name, type
    /// (e.g. "x1-fsn1", "y2-nyc3"), status, public IPs. Which cloud provider
    /// actually hosts a server is an internal detail, not exposed here. Falls
    /// back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn list_servers(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.list_servers(args.0.api_key).await)
    }

    /// List every server type available across the caller's configured
    /// provider credentials — usecode agent's own series (e.g. "x1", "y2"; no city,
    /// since specs don't vary by city) with cpu, memory, and main-disk specs.
    /// Calling this also mints a stable series for any provider type not seen
    /// before, so it can be passed to create_server afterwards. Falls
    /// back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn list_server_types(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.list_server_types(args.0.api_key).await)
    }

    /// Get one of the caller's servers by its usecode agent server id. Falls back
    /// to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn get_server(&self, args: Parameters<ServerIdArgs>) -> Json<Value> {
        let args = args.0;
        answer(self.client.get_server(&args.server_id, args.api_key).await)
    }

    /// Create a new server. `type` is usecode agent's own series-city type string,
    /// not a cloud-provider type — e.g. "x1"/"x2"/"x4"/"x8" plus a city, such
    /// as "x1-fsn1" or "x8-ash"; or "y1"/"y2"/"y4"/"y8" plus a city, such as
    /// "y1-nyc3". `image` is the OS image slug (e.g. "ubuntu-24.04"). Run
    /// sync_servers then list_catalog (kind="location" or
    /// kind="image") to see the caller's actual valid values instead of
    /// guessing. Requires credentials configured for whichever provider that
    /// type maps to, via set_provider_credentials. Provisioning runs as a background
    /// task (the server's IP and final status aren't known until the provider
    /// finishes), so this schedules the task and returns it; poll it with
    /// get_task until it 404s (meaning it finished), then use
    /// list_servers to find the new server. Falls back to the
    /// configured USECODE_MCP_API_KEY.
    #[tool]
    async fn create_server(&self, args: Parameters<CreateServerArgs>) -> Json<Value> {
        let args = args.0;
        answer(
            self.client
                .create_server(
                    &args.name,
                    &args.server_type,
                    &args.image,
                    args.ssh_keys.unwrap_or_default(),
                    args.api_key,
                )
                .await,
        )
    }

    /// Delete a server by its usecode agent server id. This is irreversible — the
    /// server and its data are destroyed. Deletion runs as a background task
    /// (the provider can take a while to tear the machine down), so this
    /// schedules the task and returns it; poll it with get_task until
    /// its state stops changing and it 404s (meaning it finished and the
    /// server is gone). Falls back to the configured USECODE_MCP_API_KEY.
    #[tool]
    async fn delete_server(&self, args: Parameters<ServerIdArgs>) -> Json<Value> {
        let args = args.0;
        answer(
            self.client
                .delete_server(&args.server_id, args.api_key)
                .await,
        )
    }

    /// List the caller's in-flight background tasks (create_server/delete_server
    /// workflows started by create_server/delete_server that
    /// haven't finished yet — a task disappears from this list once it's done,
    /// same as when get_task starts 404ing for it). Falls back to the
    /// configured USECODE_MCP_API_KEY.
    #[tool]
    async fn list_tasks(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.list_tasks(args.0.api_key).await)
    }

    /// Get the status of a background task (e.g. one started by
    /// delete_server) by its id. A 404-shaped error response means the
    /// task finished and was cleaned up. Falls back to the configured
    /// USECODE_MCP_API_KEY.
    #[tool]
    async fn get_task(&self, args: Parameters<TaskIdArgs>) -> Json<Value> {
        let args = args.0;
        answer(self.client.get_task(&args.task_id, args.api_key).await)
    }

    /// Fetch every server already provisioned with the caller's configured
    /// provider credentials and make sure each one is reflected in usecode agent's
    /// database (matched by the provider's own server id), so newly-created or
    /// externally-created servers show up in list_servers. Also
    /// mirrors each configured provider's full catalog (locations, server
    /// types, OS images) into usecode agent's database, and fixes the series/city
    /// mappings used by "x1-fsn1"/"y1-nyc3"-style type strings — see
    /// list_catalog to inspect what was stored. Falls back to the
    /// configured USECODE_MCP_API_KEY.
    #[tool]
    async fn sync_servers(&self, args: Parameters<ApiKeyArgs>) -> Json<Value> {
        answer(self.client.sync_servers(args.0.api_key).await)
    }

    /// List the provider catalog data mirrored by the most recent
    /// sync_servers call — every location, server type, and OS image
    /// each configured provider offers, as raw provider data. Optionally
    /// filter by `provider` ("hetzner"/"digitalocean") and/or `kind`
    /// ("location"/"server_type"/"image"). Use this to see valid city codes
    /// (e.g. what to put after the "-" in "x1-fsn1") and valid `image` values
    /// for create_server, instead of guessing at provider naming. Run
    /// sync_servers first if this comes back empty. Falls back to the
    /// configured USECODE_MCP_API_KEY.
    #[tool]
    async fn list_catalog(&self, args: Parameters<CatalogArgs>) -> Json<Value> {
        let args = args.0;
        answer(
            self.client
                .list_catalog(args.provider, args.kind, args.api_key)
                .await,
        )
    }

    /// Make sure usecode agent is running on this machine, starting it via deploy/compose.yml if not.
    #[tool]
    async fn ensure_running(&self) -> Json<Value> {
        Json(match compose::is_running(&self.settings).await {
            Ok(true) => json!({"status": "already running"}),
            Ok(false) => match compose::start(&self.settings).await {
                Ok(output) => status_with("started", output),
                Err(error) => json!({"error": error.to_string()}),
            },
            Err(error) => json!({"error": error.to_string()}),
        })
    }

    /// Stop usecode agent on this machine by tearing down the deploy/compose.yml stack.
    #[tool]
    async fn stop(&self) -> Json<Value> {
        Json(match compose::stop(&self.settings).await {
            Ok(output) => status_with("stopped", output),
            Err(error) => json!({"error": error.to_string()}),
        })
    }

    /// Reload usecode agent by rebuilding and recreating the deploy/compose.yml
    /// stack: `compose up -d --build --force-recreate`. Use this after making
    /// code changes (e.g. to lib/api or lib/bot) to pick them up in the
    /// running containers.
    #[tool]
    async fn reload(&self) -> Json<Value> {
        Json(match compose::start(&self.settings).await {
            Ok(output) => status_with("reloaded", output),
            Err(error) => json!({"error": error.to_string()}),
        })
    }

    /// List shell commands to follow logs for each service in deploy/compose.yml (and all of them combined).
    #[tool]
    async fn logs_commands(&self) -> Json<Value> {
        Json(match compose::logs_commands(&self.settings).await {
            Ok(commands) => commands,
            Err(error) => json!({"error": error.to_string()}),
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for UsecodeServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new("usecode agent", env!("CARGO_PKG_VERSION")),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> UsecodeServer {
        UsecodeServer::new(Arc::new(Settings::default())).unwrap()
    }

    #[test]
    fn every_python_tool_is_still_exposed() {
        let server = server();
        let names: Vec<String> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        for expected in [
            "request_otp",
            "verify_otp",
            "me",
            "logout",
            "create_api_key",
            "list_api_keys",
            "revoke_api_key",
            "health",
            "model_options",
            "model_status",
            "model_start",
            "model_stop",
            "set_provider_credentials",
            "provider_credentials_status",
            "delete_provider_credentials",
            "list_provider_credentials",
            "list_servers",
            "list_server_types",
            "get_server",
            "create_server",
            "delete_server",
            "list_tasks",
            "get_task",
            "sync_servers",
            "list_catalog",
            "ensure_running",
            "stop",
            "reload",
            "logs_commands",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
        assert_eq!(names.len(), 29);
    }

    #[test]
    fn errors_are_reported_as_a_value_not_a_failed_call() {
        let error = crate::client::ApiError::new(404, "no such task");
        let Json(value) = answer(Err(error));
        assert_eq!(value["status_code"], 404);
        assert_eq!(value["error"], "no such task");
    }
}
