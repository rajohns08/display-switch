//
// Copyright © 2020 Haim Gelfenbeyn
// This code is licensed under MIT license (see LICENSE.txt for details)
//
use crate::configuration::{Configuration, SwitchDirection};
use crate::input_source::InputSource;

use anyhow::{Error, Result};
use ddc_hi::{Ddc, Display, Handle};
use shell_words;
use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::{thread, time};

/// VCP feature code for input select
const INPUT_SELECT: u8 = 0x60;
const RETRY_DELAY_MS: u64 = 3000;

fn display_name(display: &Display, index: Option<usize>) -> String {
    // Different OSes populate different fields of ddc-hi-rs info structure differently. Create
    // a synthetic "display_name" that makes sense on each OS
    #[cfg(target_os = "linux")]
    let display_id = vec![
        &display.info.manufacturer_id,
        &display.info.model_name,
        &display.info.serial_number,
    ]
    .into_iter()
    .flatten()
    .map(|s| s.as_str())
    .collect::<Vec<&str>>()
    .join(" ");
    #[cfg(target_os = "macos")]
    let display_id = &display.info.id;
    #[cfg(target_os = "windows")]
    let display_id = &display.info.id;

    if let Some(index) = index {
        format!("'{} #{}'", display_id, index)
    } else {
        format!("'{}'", display_id)
    }
}

fn are_display_names_unique(displays: &[Display]) -> bool {
    let mut hash = HashSet::new();
    displays.iter().all(|display| hash.insert(display_name(display, None)))
}

fn try_switch_display(handle: &mut Handle, display_name: &str, input: InputSource) {
	match handle.get_vcp_feature(INPUT_SELECT) {
		Ok(raw_source) => {
			if raw_source.value() & 0xff == input.value() {
				info!("Display {} is already set to {}", display_name, input);
				return;
			}
		}
		Err(err) => {
			warn!("Failed to get current input for display {}: {:?}", display_name, err);
		}
	}
	debug!("Setting display {} to {}", display_name, input);
	match handle.set_vcp_feature(INPUT_SELECT, input.value()) {
		Ok(_) => {
			info!("Display {} set to {}", display_name, input);
		}
		Err(err) => {
			error!("Failed to set display {} to {} ({:?})", display_name, input, err);
		}
	}
}

pub fn switch(config: &Configuration, switch_direction: SwitchDirection) {
    if let Some(execute_command) = config.default_input_sources.execute_command(switch_direction) {
        run_command(execute_command)
    }
}

fn run_command(execute_command: &str) {
    fn try_run_command(execute_command: &str) -> Result<()> {
        let mut arguments = shell_words::split(execute_command)?;
        if arguments.is_empty() {
            return Ok(());
        }

        let executable = arguments.remove(0);
        let output = Command::new(executable).args(arguments).stdin(Stdio::null()).output()?;
        return if output.status.success() {
            info!("External command '{}' executed successfully", execute_command);
            Ok(())
        } else {
            let msg = if let Some(code) = output.status.code() {
                format!("Exited with status {}\n", code)
            } else {
                "Exited because of a signal\n".to_string()
            };
            let stdout = if !output.stdout.is_empty() {
                if let Ok(s) = String::from_utf8(output.stdout) {
                    format!("Stdout = [{}]\n", s)
                } else {
                    format!("Stdout was not UTF-8")
                }
            } else {
                "No stdout\n".to_string()
            };
            let stderr = if !output.stderr.is_empty() {
                if let Ok(s) = String::from_utf8(output.stderr) {
                    format!("Stderr = [{}]\n", s)
                } else {
                    format!("Stderr was not UTF-8")
                }
            } else {
                "No stderr\n".to_string()
            };
            Err(Error::msg(format!("{} {} {}", msg, stdout, stderr)))
        };
    }

    try_run_command(execute_command)
        .unwrap_or_else(|err| error!("Error executing external command '{}': {}", execute_command, err))
}
