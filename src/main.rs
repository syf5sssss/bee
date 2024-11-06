use std::process::Command;
use serde::{ Deserialize, Serialize };
use tokio::net::UdpSocket;
use std::fs::File;
use std::io::{ Read, Write };
use serde_yaml;
use std::path::Path;
use sysinfo::System;
use serde_json::{ json, Value };
use if_addrs::get_if_addrs;
use std::net::IpAddr;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    qport: String,
    bport: String,
    name: String,
    info: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = load_config("config.yaml").await?;
    println!("----------------- Start ------------------");
    println!("Loaded config: {:?}", config);

    let mut sys = System::new_all();
    sys.refresh_all();

    let article: Vec<String> = vec![
        format!("bee:"),
        format!("名称: {}", config.name),
        format!("描述: {}", config.info),
        format!(
            "系统名称: {}",
            System::name().unwrap_or_else(|| "Unknown".to_string())
        ),
        format!(
            "系统内核版本: {}",
            System::kernel_version().unwrap_or_else(|| "Unknown".to_string())
        ),
        format!(
            "系统操作系统版本: {}",
            System::os_version().unwrap_or_else(|| "Unknown".to_string())
        ),
        format!(
            "系统主机名: {}",
            System::host_name().unwrap_or_else(|| "Unknown".to_string())
        )
    ];

    let socket = UdpSocket::bind(&format!("0.0.0.0:{}", config.bport)).await?;
    socket.set_broadcast(true).expect("Could not set broadcast option");
    println!("{}", format!("UDP Server listening on port {}", config.bport));
    let mut buf = [0; 1024];
    while let Ok((size, addr)) = socket.recv_from(&mut buf).await {
        let received_message = String::from_utf8_lossy(&buf[..size]);
        println!("Received from {}: {}", addr, received_message);
        let mut jsonres: Value = json!({});
        let mut arr: Vec<String> = Vec::new();
        // jsonres["ip"] = json!(addr.ip());
        jsonres["data"] = json!([]);
        let res = received_message;

        if res.eq("hello") {
            jsonres["data"] = json!(article);
            broadcast(&socket, &jsonres.to_string(), &config.qport).await?;
        }
        if res.contains("ips") {
            let trimmed = res.trim_start_matches("ips:");
            if trimmed.len() > 0 && !trimmed.is_empty() {
                if trimmed.eq(&config.name) {
                    match get_if_addrs() {
                        Ok(if_addrs) => {
                            for if_addr in if_addrs {
                                let name = if_addr.name.clone();
                                let ip = if_addr.ip();
                                arr.push(format!("Interface: {}", name));
                                match ip {
                                    IpAddr::V4(ipv4) => {
                                        arr.push(format!("  IPv4 Address: {}", ipv4));
                                    }
                                    IpAddr::V6(ipv6) => {
                                        arr.push(format!("  IPv6 Address: {}", ipv6));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            arr.push(format!("Error getting interface addresses: {}", e));
                        }
                    }
                    jsonres["data"] = json!(arr);
                    broadcast(&socket, &jsonres.to_string(), &config.qport).await?;
                }
            }
        }
        if res.contains("cmd:") {
            let trimmed = res.trim_start_matches("cmd:");
            if trimmed.len() > 0 && !trimmed.is_empty() {
                if let Some(colon_index) = trimmed.find(':') {
                    let (first_part, second_part) = trimmed.split_at(colon_index);
                    if
                        first_part.len() > 0 &&
                        !first_part.is_empty() &&
                        second_part.len() > 0 &&
                        !second_part.is_empty()
                    {
                        if first_part.eq(&config.name) {
                            let second_part_without_colon = second_part.trim_start_matches(':');
                            let cmdres = run_command(second_part_without_colon);
                            match cmdres {
                                Ok(value) => {
                                    let lines: Vec<&str> = value.split('\n').collect();
                                    jsonres["data"] = json!(lines);
                                    broadcast(&socket, &jsonres.to_string(), &config.qport).await?;
                                }
                                Err(e) => {
                                    let error_message = e.to_string();
                                    let lines: Vec<&str> = error_message.split('\n').collect();
                                    jsonres["data"] = json!(lines);
                                    broadcast(&socket, &jsonres.to_string(), &config.qport).await?;
                                }
                            }
                        }
                    }
                }
            }
        }
        if res.contains("info:") {
            let trimmed = res.trim_start_matches("info:");
            if trimmed.len() > 0 && !trimmed.is_empty() {
                if let Some(colon_index) = trimmed.find(':') {
                    let (first_part, second_part) = trimmed.split_at(colon_index);
                    if
                        first_part.len() > 0 &&
                        !first_part.is_empty() &&
                        second_part.len() > 0 &&
                        !second_part.is_empty()
                    {
                        if first_part.eq(&config.name) {
                            let second_part_without_colon = second_part.trim_start_matches(':');
                            config.info = second_part_without_colon.to_string();
                            save_config("config.yaml", &config).await?;
                            let lines: Vec<&str> = config.info.split('\n').collect();
                            jsonres["data"] = json!(lines);
                            broadcast(&socket, &jsonres.to_string(), &config.qport).await?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn broadcast(
    socket: &UdpSocket,
    message: &str,
    port: &str
) -> Result<(), Box<dyn std::error::Error>> {
    let broadcast_addr = &format!("255.255.255.255:{}", port);
    socket.send_to(message.as_bytes(), broadcast_addr).await?;
    println!("Send: {}, from {}", message, broadcast_addr);
    Ok(())
}

async fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let path = Path::new(path);
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let config: Config = serde_yaml::from_str(&contents)?;
    Ok(config)
}

async fn save_config(path: &str, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(path);
    let mut file = File::create(path)?;
    let contents = serde_yaml::to_string(config)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

fn run_command(command: &str) -> Result<String, String> {
    let shell = if cfg!(target_os = "windows") { "cmd" } else { "sh" };
    let output = Command::new(shell)
        .arg(if shell == "cmd" { "/c" } else { "-c" })
        .arg(command)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
