use std::env;
use std::path::Path;
use crate::protocol::models::*;
use crate::state::AppState;

pub fn build_server_config(state: &AppState) -> ServerConfig {
    let cwd = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let keybindings_config_path = state
        .home_dir
        .join("keybindings.json")
        .display()
        .to_string();

    let providers = get_server_providers(state);

    ServerConfig {
        environment: ExecutionEnvironmentDescriptor {
            environment_id: state.environment_id.clone(),
            label: "t1.73code".to_string(),
            platform: ExecutionEnvironmentPlatform {
                os: env::consts::OS.to_string(),
                arch: env::consts::ARCH.to_string(),
                machine: "local".to_string(),
            },
            server_version: state.server_version.clone(),
            orchestration_protocol_version: Some(2),
            database_migration: Some(1),
            capabilities: ExecutionEnvironmentCapabilities {
                voice_transcription: Some(false),
                cloud_voice_transcription: Some(false),
            },
        },
        auth: ServerAuthDescriptor {
            active_methods: vec!["bearer-access-token".to_string()],
            token_exchange: false,
        },
        cwd,
        keybindings_config_path,
        keybindings: ResolvedKeybindingsConfig { bindings: vec![] },
        issues: vec![],
        providers,
        available_editors: vec!["code".to_string(), "cursor".to_string(), "vim".to_string()],
        observability: ServerObservability { enabled: false },
        settings: ServerSettings {
            default_provider: Some("pi".to_string()),
            default_model: Some("default".to_string()),
        },
        shell_thread_families: Some(true),
        thread_link_metadata: Some(true),
        thread_snapshot_pagination: Some(true),
    }
}

pub fn get_server_providers(_state: &AppState) -> Vec<ServerProvider> {
    let pi_available = check_pi_available();
    vec![ServerProvider {
        instance_id: "pi".to_string(),
        driver: "pi".to_string(),
        display_name: Some("Pi Agent".to_string()),
        models: vec![ServerProviderModel {
            id: "default".to_string(),
            label: Some("Pi Default Model".to_string()),
        }],
        enabled: true,
        status: ServerProviderStatus {
            status: if pi_available {
                "ready".to_string()
            } else {
                "not_installed".to_string()
            },
            message: if pi_available {
                None
            } else {
                Some("Pi CLI binary not found in PATH".to_string())
            },
        },
        runtime_modes: Some(vec!["autonomous".to_string()]),
        slash_commands: Some(vec![]),
        skills: Some(vec![]),
    }]
}

pub fn check_pi_available() -> bool {
    if let Ok(prog) = std::env::var("PI_PROGRAM_PATH") {
        if Path::new(&prog).is_file() {
            return true;
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for path in std::env::split_paths(&path_var) {
            let pi_path = path.join("pi");
            if pi_path.is_file() {
                return true;
            }
        }
    }
    false
}
