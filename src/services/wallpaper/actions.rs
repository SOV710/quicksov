// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{Map, Value};

use crate::bus::ServiceError;
use crate::util::is_empty_object;

use super::model::{CropRect, WallpaperState};

#[derive(Debug)]
pub(super) enum WallpaperAction {
    Refresh,
    SetOutputSource {
        output: String,
        source: String,
    },
    SetOutputPath {
        output: String,
        path: String,
    },
    StepOutput {
        output: String,
        step: isize,
    },
    SetOutputCrop {
        output: String,
        crop: Option<CropRect>,
    },
}

impl WallpaperAction {
    pub(super) fn parse(action: &str, payload: &Value) -> Result<Self, ServiceError> {
        match action {
            "refresh" => {
                if !is_empty_object(payload) {
                    return Err(ServiceError::ActionPayload {
                        msg: "refresh expects an empty object payload".to_string(),
                    });
                }
                Ok(Self::Refresh)
            }
            "set_output_source" => {
                let obj = action_object(payload, "set_output_source expects an object payload")?;
                Ok(Self::SetOutputSource {
                    output: string_field(obj, "output", "set_output_source requires `output`")?
                        .to_string(),
                    source: string_field(obj, "source", "set_output_source requires `source`")?
                        .to_string(),
                })
            }
            "set_output_path" => {
                let obj = action_object(payload, "set_output_path expects an object payload")?;
                Ok(Self::SetOutputPath {
                    output: string_field(obj, "output", "set_output_path requires `output`")?
                        .to_string(),
                    path: string_field(obj, "path", "set_output_path requires `path`")?.to_string(),
                })
            }
            "next_output" | "prev_output" => {
                let obj =
                    action_object(payload, "next_output/prev_output expect an object payload")?;
                Ok(Self::StepOutput {
                    output: string_field(
                        obj,
                        "output",
                        "next_output/prev_output require `output`",
                    )?
                    .to_string(),
                    step: if action == "next_output" { 1 } else { -1 },
                })
            }
            "set_output_crop" => {
                let obj = action_object(payload, "set_output_crop expects an object payload")?;
                let crop = match obj.get("crop") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(parse_crop(value)?),
                };
                Ok(Self::SetOutputCrop {
                    output: string_field(obj, "output", "set_output_crop requires `output`")?
                        .to_string(),
                    crop,
                })
            }
            other => Err(ServiceError::ActionUnknown {
                action: other.to_string(),
            }),
        }
    }

    pub(super) fn apply(self, state: &mut WallpaperState) -> Result<Value, ServiceError> {
        match self {
            Self::Refresh => {
                state.rescan();
                if state.is_ready() {
                    Ok(Value::Null)
                } else {
                    Err(ServiceError::Unavailable)
                }
            }
            Self::SetOutputSource { output, source } => {
                state.set_output_source(&output, &source)?;
                Ok(Value::Null)
            }
            Self::SetOutputPath { output, path } => {
                state.set_output_path(&output, &path)?;
                Ok(Value::Null)
            }
            Self::StepOutput { output, step } => {
                state.step_output(&output, step)?;
                Ok(Value::Null)
            }
            Self::SetOutputCrop { output, crop } => {
                state.set_output_crop(&output, crop)?;
                Ok(Value::Null)
            }
        }
    }
}

fn parse_crop(value: &Value) -> Result<CropRect, ServiceError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: "crop must be an object or null".to_string(),
        })?;

    let crop = CropRect {
        x: number_field(obj, "x", "crop.x is required")?,
        y: number_field(obj, "y", "crop.y is required")?,
        width: number_field(obj, "width", "crop.width is required")?,
        height: number_field(obj, "height", "crop.height is required")?,
    };

    if crop.is_valid() {
        Ok(crop)
    } else {
        Err(ServiceError::ActionPayload {
            msg: "crop must be normalized to 0..1 and stay inside bounds".to_string(),
        })
    }
}

fn action_object<'a>(
    payload: &'a Value,
    message: &str,
) -> Result<&'a Map<String, Value>, ServiceError> {
    payload
        .as_object()
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: message.to_string(),
        })
}

fn string_field<'a>(
    obj: &'a Map<String, Value>,
    key: &str,
    message: &str,
) -> Result<&'a str, ServiceError> {
    let value =
        obj.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| ServiceError::ActionPayload {
                msg: message.to_string(),
            })?;
    if value.is_empty() {
        return Err(ServiceError::ActionPayload {
            msg: message.to_string(),
        });
    }
    Ok(value)
}

fn number_field(obj: &Map<String, Value>, key: &str, message: &str) -> Result<f64, ServiceError> {
    obj.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: message.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::WallpaperAction;

    #[test]
    fn parses_next_output_action() {
        let action = WallpaperAction::parse("next_output", &json!({"output": "HDMI-A-3"}))
            .expect("action should parse");
        assert!(matches!(
            action,
            WallpaperAction::StepOutput {
                output,
                step: 1
            } if output == "HDMI-A-3"
        ));
    }

    #[test]
    fn rejects_out_of_bounds_crop() {
        let err = WallpaperAction::parse(
            "set_output_crop",
            &json!({
                "output": "HDMI-A-3",
                "crop": {
                    "x": 0.8,
                    "y": 0.0,
                    "width": 0.5,
                    "height": 1.0
                }
            }),
        )
        .expect_err("crop should be rejected");

        assert!(err.to_string().contains("normalized"));
    }
}
