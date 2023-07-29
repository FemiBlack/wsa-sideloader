// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::Value;
use thiserror::Error;

use regex::Regex;
use std::{
    ffi::OsStr,
    fs,
    io::{BufRead, BufReader},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
};

use tauri::Wry;
use tauri_plugin_store::{with_store, StoreCollection};

// Custom error type that represents all errors possible in our program
#[derive(Debug, Error)]
enum CustomError {
    #[error("Package name not found")]
    PackageNameNotFound,

    // #[error("Package already installed")]
    // PackageAlreadyInstalled,
    #[error("Failed to install application: {0}")]
    InstallError(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    OtherError(String),
}

// Implement Serialize trait for MyError to make it serializable
impl serde::Serialize for CustomError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// Implement conversion from &str to CustomError
impl From<&str> for CustomError {
    fn from(err: &str) -> Self {
        CustomError::OtherError(err.to_string())
    }
}

// Implement conversion from String to CustomError
impl From<String> for CustomError {
    fn from(err: String) -> Self {
        CustomError::OtherError(err)
    }
}

#[derive(Debug, serde::Serialize)]
struct PackageInfo {
    name: String,
    version_name: String,
    version_code: String,
    label: String,
}

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[tauri::command]
fn list_apk_files(path: &str) -> Result<Vec<String>, tauri::Error> {
    // Initialize an empty vector to store the paths of APK files
    let mut result = vec![];

    // Loop through the entries in the given directory
    for path in fs::read_dir(path).unwrap() {
        // `read_dir` returns a Result<fs::DirEntry>, so we need to handle any potential errors.
        let path = path.unwrap().path();
        // Check if the file extension is "apk" (case-sensitive).
        // If it is, push the path into the result vector.
        if let Some("apk") = path.extension().and_then(OsStr::to_str) {
            result.push(path.to_string_lossy().to_string());
        }
    }
    Ok(result)
}

fn read_from_store(
    app_handle: tauri::AppHandle,
    key: &str,
) -> Result<Option<Value>, tauri_plugin_store::Error> {
    let stores = tauri::Manager::state::<StoreCollection<Wry>>(&app_handle);
    let path = PathBuf::from(".settings.dat");
    let result = with_store(app_handle.clone(), stores, path, |store| {
        let docs = store.get(key);
        Ok(docs.cloned())
    });

    result
}

#[tauri::command]
fn connect_adb(app: tauri::AppHandle) -> Result<Vec<u8>, String> {
    // Read the IP address from the store
    let ip_address = match read_from_store(app.clone(), "host-address") {
        Ok(Some(value)) => {
            // Convert the JsonValue to a string
            let ip_str = value.as_str().ok_or("Invalid IP address in store")?;
            // Remove any leading or trailing double quotes, if present
            let ip_str = ip_str.trim_matches('"');
            ip_str.to_string()
        }
        _ => {
            let message = "Host Address not found";
            return Err(message.to_string());
        }
    };
    let is_connected = check_if_connected_to_host(&ip_address);
    if is_connected {
        // Do nothing
        return Ok(Vec::new());
    };
    let command_prompt_cmd = format!("adb connect {}", ip_address.trim());

    let output = Command::new("cmd")
        .args(&["/C", &command_prompt_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("failed to execute process");
    if output.status.success() {
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        println!("Output is {:?}", stdout_str);
        Ok(output.stdout)
    } else {
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        eprintln!("Error executing command: {:?}", stderr_str);
        Err(stderr_str)
    }
}

#[tauri::command]
fn install_application(path: &str) -> Result<Vec<u8>, CustomError> {
    let package_name = find_package_name(path).ok_or(CustomError::PackageNameNotFound)?;
    let package_version = get_apk_package_version(&path).ok_or(CustomError::InstallError(
        "Package Version not found".to_string(),
    ))?;
    println!("{} {}", &package_name, &package_version);
    if check_if_package_installed(&package_name, &package_version) {
        let message = format!(
            "Package already installed: {} version: {}",
            &package_name, &package_version
        );
        let message_bytes = message.as_bytes().to_vec();
        return Ok(message_bytes);
    }
    let output = Command::new("adb")
        .arg("install")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        // .expect("failed to execute process")
        .map_err(|e| CustomError::InstallError(format!("Failed to execute process: {}", e)))?;
    if output.status.success() {
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        println!("Output is {:?}", stdout_str);
        Ok(output.stdout)
    } else {
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        eprintln!("Error executing command: {:?}", stderr_str);
        Err(CustomError::InstallError(stderr_str))
    }
}

fn find_package_name(apk_path: &str) -> Option<String> {
    // Construct the PowerShell command
    let powershell_cmd = format!(
        "aapt dump badging '{}' | Select-String -Pattern 'package: name=''([^'']+)' | ForEach-Object {{ $_.Matches.Groups[1].Value }}",
        apk_path
    );

    // Run the PowerShell command
    let output = Command::new("powershell")
        .arg("-command")
        .arg(powershell_cmd)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("Failed to execute PowerShell process");

    // Check if the command was successful
    if output.status.success() {
        let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("Output is {:?}", stdout_str);
        Some(stdout_str)
    } else {
        let stderr_str = String::from_utf8_lossy(&output.stderr).trim().to_string();
        println!("PowerShell command failed: {:?}", stderr_str);
        None
    }
}

#[tauri::command]
fn check_if_connected_to_host(host_address: &str) -> bool {
    let command_prompt_cmd = "adb devices";

    // Run the Command Prompt command and capture stdout
    let process = Command::new("cmd")
        .args(&["/C", command_prompt_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("Failed to execute command prompt process");

    let devices_output = String::from_utf8_lossy(&process.stdout);
    devices_output.contains(&format!("{}\tdevice", host_address))
}

fn get_apk_package_version(apk_path: &str) -> Option<String> {
    // Construct the PowerShell command
    let powershell_cmd = format!(
        "aapt dump badgAing '{}' | Select-String -Pattern 'versionName=''([^'']+)' | ForEach-Object {{ $_.Matches.Groups[1].Value }}",
        apk_path
    );

    // Run the PowerShell command
    let output = Command::new("powershell")
        .arg("-command")
        .arg(powershell_cmd)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("Failed to execute PowerShell process");

    // Check if the command was successful
    if output.status.success() {
        let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("Output is {:?}", stdout_str);
        Some(stdout_str)
    } else {
        let stderr_str = String::from_utf8_lossy(&output.stderr).trim().to_string();
        println!("PowerShell command failed: {:?}", stderr_str);
        None
    }
}
fn push_aapt_arm_pie_to_device(local_path: &str, device_path: &str) -> Result<(), CustomError> {
    // Push the binary to the device
    let push_cmd = Command::new("adb")
        .args(&["push", local_path, device_path])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !push_cmd.status.success() {
        return Err("Failed to push aapt-arm-pie to the device.".into());
    }

    // Set the correct permissions on the binary
    let chmod_cmd = Command::new("adb")
        .args(&["shell", "chmod", "0755", device_path])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !chmod_cmd.status.success() {
        return Err("Failed to set permissions for aapt-arm-pie on the device.".into());
    }

    Ok(())
}

fn check_aapt_arm_pie_exists() -> Result<bool, CustomError> {
    let output = Command::new("adb")
        .args(&["shell", "ls", "/data/local/tmp/"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(CustomError::from)?;

    let aapt_exists = String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim() == "aapt-arm-pie");

    Ok(aapt_exists)
}

fn get_package_info(package: &str) -> Result<PackageInfo, CustomError> {
    if !check_aapt_arm_pie_exists()? {
        // TODO: AppResolve aapt-arm-pie bundled resource
        push_aapt_arm_pie_to_device("path\\to\\arm-pie", "/data/local/tmp/")?;
    }
    let aapt_binary = "/data/local/tmp/aapt-arm-pie";
    let package_info_cmd = format!("d badging {}", package);

    let output = Command::new("adb")
        .args(&["shell", aapt_binary, &package_info_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    let package_output = String::from_utf8_lossy(&output.stdout);

    let mut info = PackageInfo {
        name: String::new(),
        version_code: String::new(),
        version_name: String::new(),
        label: String::new(),
    };

    for line in package_output.lines() {
        if line.starts_with("package: name") {
            info.name = line.split('\'').nth(1).unwrap_or("").to_string();
            info.version_code = line.split('\'').nth(3).unwrap_or("").to_string();
            info.version_name = line.split('\'').nth(5).unwrap_or("").to_string();
        } else if line.starts_with("application-label") {
            info.label = line.split('\'').nth(1).unwrap_or("").to_string();
        }
    }

    if info.name.is_empty()
        || info.version_code.is_empty()
        || info.version_name.is_empty()
        || info.label.is_empty()
    {
        return Err(format!("Package info not found for: {}", package).into());
    } else {
        Ok(info)
    }
}

fn list_third_party_packages() -> Result<Vec<String>, CustomError> {
    let output = Command::new("adb")
        .args(&["shell", "pm list packages -3 -f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if !output.status.success() {
        return Err("Failed to execute adb command.".into());
    }

    let packages_output = String::from_utf8_lossy(&output.stdout);
    let mut packages = Vec::new();
    let regex = Regex::new(r"package:(.+?/base\.apk)").unwrap();

    for caps in regex.captures_iter(&packages_output) {
        let package_path = &caps[1];
        packages.push(package_path.trim().to_string());
    }

    Ok(packages)
}

#[tauri::command]
fn get_all_third_party_package_info() -> Result<Vec<PackageInfo>, CustomError> {
    let packages = list_third_party_packages()?;
    let mut package_info_list = Vec::new();

    for package in packages {
        match get_package_info(&package) {
            Ok(info) => package_info_list.push(info),
            Err(err) => {
                println!("Error processing package {}: {:?}", package, err);
            }
        }
    }

    Ok(package_info_list)
}

fn get_installed_package_version(package_name: &str) -> Result<String, String> {
    let output = Command::new("adb")
        .args(&["shell", "dumpsys", "package", package_name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("Failed to execute adb command");

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if line.contains("versionName") {
                if let Some(version) = line.split("=").nth(1) {
                    return Ok(version.trim().to_string());
                }
            }
        }
        return Err("versionName not found in adb output".to_string());
    } else {
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("Error executing command: {}", stderr_str));
    }
}
fn check_if_package_installed(package_name: &str, expected_version: &str) -> bool {
    if check_if_package_in_app_list(package_name) {
        if let Ok(package_version) = get_installed_package_version(package_name) {
            println!(
                "package v:{}, expected: {}",
                &package_version, &expected_version
            );
            if package_version == expected_version {
                return true;
            }
        }
    }
    false
}

fn check_if_package_in_app_list(package_name: &str) -> bool {
    // Construct the adb command to check if the package is installed
    let adb_command = format!("adb shell pm list packages {}", package_name);

    // Run the adb command and capture stdout
    let process = Command::new("cmd")
        .args(&["/C", &adb_command])
        .stdout(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .expect("Failed to execute command prompt process");

    let stdout = process.stdout.expect("Failed to capture stdout");
    let reader = BufReader::new(stdout);

    // Check if the package is installed
    for line in reader.lines() {
        if let Ok(line_content) = line {
            if line_content == format!("package:{}", package_name) {
                return true;
            }
        }
    }

    false
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_apk_files,
            install_application,
            connect_adb,
            check_if_connected_to_host,
            get_all_third_party_package_info
        ])
        .plugin(tauri_plugin_store::Builder::default().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
