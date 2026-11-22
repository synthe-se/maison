//! Actions: the device orders a scene runs, one after the other (`run`), checked when a scene
//! is saved (`validate`). The types are `maison_api::actions`'.

use crate::error::OrRefused;
use std::future::Future;

use maison_api::lamps::{HueLamp, ZigbeeLamp};

use crate::{
    AppState,
    error::{AppError, Refusal},
    matter::CoverCommand,
    mitsubishi_ir::{self, ClimateSettings},
    routes::lamps::LampBackend,
    tv,
    util::refused,
};

pub use maison_api::actions::{Action, ActionResult, CoverOrder, SwitchState};

/// Whether a switch-like action leaves the device on.
pub(crate) trait Resolve {
    /// Whether the device should end up on. `current` (is it on now?) is only asked for a
    /// toggle, so forcing a state costs no round trip.
    async fn resolve<F>(self, current: impl FnOnce() -> F) -> Result<bool, AppError>
    where
        F: Future<Output = Result<bool, AppError>>;
}

impl Resolve for SwitchState {
    async fn resolve<F>(self, current: impl FnOnce() -> F) -> Result<bool, AppError>
    where
        F: Future<Output = Result<bool, AppError>>,
    {
        match self {
            Self::On => Ok(true),
            Self::Off => Ok(false),
            Self::Toggle => Ok(!current().await?),
        }
    }
}

/// Checked when a scene is saved, so a wrong action fails then and not at its run: the coded
/// `invalid` refusal, its reason in the log.
pub fn validate(actions: &[Action]) -> Result<(), AppError> {
    match invalid_reason(actions) {
        Some(reason) => Err(refused(Refusal::Invalid, reason)),
        None => Ok(()),
    }
}

/// Why `actions` cannot be kept, if they cannot.
fn invalid_reason(actions: &[Action]) -> Option<String> {
    actions.iter().find_map(|action| match action {
        Action::AndroidTvApp { package, .. } => crate::androidtv::validate_package(package)
            .is_err()
            .then(|| {
                format!("invalid Android package {package:?} (expected e.g. org.smarttube.beta)")
            }),
        Action::ClimateToggle { on_command, .. } => {
            mitsubishi_ir::parse_climate_settings(on_command)
                .is_none()
                .then(|| {
                    format!(
                        "invalid climate onCommand {on_command:?} (expected e.g. \
                         state-cool-16-fan-4-vane-swing, and not state-off)"
                    )
                })
        }
        Action::Cover {
            command, position, ..
        } => cover_command(*command, *position).err(),
        Action::HueBrightness { brightness, .. } | Action::ZigbeeBrightness { brightness, .. } => {
            (!(1..=100).contains(brightness))
                .then(|| format!("invalid brightness {brightness} (expected 1 to 100: 0 is off)"))
        }
        _ => None,
    })
}

/// Runs every action in order and says how each went; a failing action never stops the
/// others. Why one failed goes to the log; the result says its refusal's code.
pub async fn run(state: &AppState, actions: &[Action]) -> Vec<ActionResult> {
    let mut results = Vec::with_capacity(actions.len());
    for action in actions {
        results.push(result(action, execute(state, action).await));
    }
    results
}

fn result(action: &Action, outcome: Result<(), AppError>) -> ActionResult {
    match outcome {
        Ok(()) => ActionResult {
            action: action.clone(),
            ok: true,
            code: None,
        },
        Err(error) => {
            tracing::warn!(?action, %error, "action failed");
            ActionResult {
                action: action.clone(),
                ok: false,
                code: Some(code_of(&error).as_str().to_string()),
            }
        }
    }
}

/// The refusal an error stands for: its own code, or the nearest one by what it is.
fn code_of(error: &AppError) -> Refusal {
    use axum::http::StatusCode;
    match error {
        AppError::Coded { code, .. } => Refusal::parse(code).unwrap_or(Refusal::Internal),
        AppError::Http { status, .. } => match *status {
            StatusCode::NOT_FOUND => Refusal::NotFound,
            StatusCode::CONFLICT => Refusal::Busy,
            StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT => Refusal::DeviceUnreachable,
            s if s.is_client_error() => Refusal::Invalid,
            _ => Refusal::Internal,
        },
        AppError::Reqwest(_) => Refusal::DeviceUnreachable,
        AppError::Io(_) | AppError::Json(_) | AppError::Jwt(_) | AppError::Join(_) => {
            Refusal::Internal
        }
    }
}

/// The Matter command a `cover` action stands for, or why it stands for none.
fn cover_command(order: CoverOrder, position: Option<u8>) -> Result<CoverCommand, String> {
    match (order, position) {
        (CoverOrder::Position, Some(percent @ 0..=100)) => Ok(CoverCommand::OpenPercent(percent)),
        (CoverOrder::Position, Some(percent)) => Err(format!(
            "invalid cover position {percent} (expected 0 to 100)"
        )),
        (CoverOrder::Position, None) => {
            Err("a cover position needs its position (0 to 100)".to_string())
        }
        (_, Some(_)) => Err("only a cover position takes a position".to_string()),
        (CoverOrder::Open, None) => Ok(CoverCommand::Open),
        (CoverOrder::Close, None) => Ok(CoverCommand::Close),
        (CoverOrder::Stop, None) => Ok(CoverCommand::Stop),
    }
}

/// `zigbee_power` and `hue_power`: one lamp of either family on, off or flipped (`is_on`
/// reads the family's lamp).
async fn lamp_power<B: LampBackend>(
    lamps: &B,
    lamp: &str,
    switch: SwitchState,
    is_on: fn(&B::View) -> bool,
) -> Result<(), AppError> {
    let on = switch
        .resolve(|| async {
            let view = lamps.get(lamp).await.or_not_found()?;
            Ok(is_on(&view))
        })
        .await?;
    lamps.set_power(lamp, on).await?;
    Ok(())
}

/// `zigbee_brightness` and `hue_brightness`.
async fn lamp_brightness<B: LampBackend>(
    lamps: &B,
    lamp: &str,
    brightness: u8,
) -> Result<(), AppError> {
    lamps.set_brightness(lamp, brightness).await?;
    Ok(())
}

/// The settings `climate_on` sends: the last ones given, without their sleep timer.
fn resumed(last: Option<ClimateSettings>) -> Result<ClimateSettings, AppError> {
    let last = last.or_not_configured()?;
    Ok(ClimateSettings {
        stop_in_minutes: None,
        ..last
    })
}

/// Sends one Mitsubishi command; the Broadlink manager records the AC's new state (on with
/// these settings, or off).
async fn climate(
    state: &AppState,
    host: &str,
    command: String,
    model: &Option<String>,
) -> Result<(), AppError> {
    state
        .broadlink
        .send_mitsubishi_command(host.to_string(), None, command, model.clone())
        .await?;
    Ok(())
}

/// One device action.
async fn execute(state: &AppState, action: &Action) -> Result<(), AppError> {
    match action {
        Action::Nabaztag { command } => state.nabaztag.send_command(command).await,
        Action::ZigbeePower {
            lamp,
            state: switch,
        } => lamp_power(&state.zigbee, lamp, *switch, |l: &ZigbeeLamp| l.state.is_on).await,
        Action::ZigbeeBrightness { lamp, brightness } => {
            lamp_brightness(&state.zigbee, lamp, *brightness).await
        }
        Action::HuePower {
            lamp,
            state: switch,
        } => lamp_power(&state.hue, lamp, *switch, |l: &HueLamp| l.state.is_on).await,
        Action::HueBrightness { lamp, brightness } => {
            lamp_brightness(&state.hue, lamp, *brightness).await
        }
        Action::Cover {
            cover,
            command,
            position,
        } => {
            let order =
                cover_command(*command, *position).map_err(|r| refused(Refusal::Invalid, r))?;
            state.matter.command(cover, order).await?;
            Ok(())
        }
        Action::BroadlinkCode { host, code_id } => {
            state
                .broadlink
                .send_saved_code(host.clone(), None, code_id.clone())
                .await?;
            Ok(())
        }
        Action::MerossPower {
            device,
            state: switch,
        } => {
            let on = switch.resolve(|| state.meross.is_on(device)).await?;
            state.meross.toggle(device, on).await?;
            Ok(())
        }
        Action::TvPower {
            state: switch,
            switch_to_box,
        } => {
            tv::tv_power(state, *switch, *switch_to_box).await?;
            Ok(())
        }
        Action::TvKey { key } => state.tv.send_key(*key).await,
        Action::TvVolume { level } => {
            state.tv.set_volume(Some(*level), None).await?;
            Ok(())
        }
        Action::TvAmbilight { state: switch } => {
            let on = switch
                .resolve(|| async { Ok(state.tv.ambilight().await?.power) })
                .await?;
            state.tv.set_ambilight_power(on).await?;
            Ok(())
        }
        Action::AndroidTvApp {
            package,
            ensure_tv_on,
        } => crate::androidtv::launch_with_tv(state, package, *ensure_tv_on).await,
        Action::AndroidTvKey { key } => state.androidtv.send_key(*key).await,
        Action::ClimateToggle {
            host,
            on_command,
            model,
        } => {
            // IR is one-way: the stored state is the last commanded one.
            let is_on = state
                .broadlink
                .climate_state()
                .await
                .is_some_and(|s| s.power);
            let command = if is_on {
                mitsubishi_ir::OFF_COMMAND
            } else {
                on_command.as_str()
            };
            climate(state, host, command.to_string(), model).await
        }
        Action::ClimateOff { host, model } => {
            let off = mitsubishi_ir::OFF_COMMAND.to_string();
            climate(state, host, off, model).await
        }
        Action::ClimateOn { host, model } => {
            let settings = resumed(
                state
                    .broadlink
                    .climate_state()
                    .await
                    .and_then(|s| s.settings),
            )?;
            let command = settings
                .command()
                .map_err(|r| refused(Refusal::Invalid, r))?;
            climate(state, host, command, model).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_climate_toggle_is_checked_when_saved() {
        let toggle = |on_command: &str| {
            vec![Action::ClimateToggle {
                host: "h".to_string(),
                on_command: on_command.to_string(),
                model: None,
            }]
        };
        assert!(validate(&toggle("state-cool-16-fan-4-vane-swing")).is_ok());
        // state-off as onCommand makes the toggle a no-op: refused at save.
        assert!(validate(&toggle(mitsubishi_ir::OFF_COMMAND)).is_err());
        assert!(
            validate(&toggle("state-cool-99-fan-4-vane-swing")).is_err(),
            "temp out of range"
        );
    }

    #[test]
    fn an_app_is_checked_when_saved() {
        let app = |package: &str| {
            vec![Action::AndroidTvApp {
                package: package.to_string(),
                ensure_tv_on: true,
            }]
        };
        assert!(validate(&app("org.smarttube.beta")).is_ok());
        assert!(validate(&app("rm -rf /")).is_err());
    }

    #[test]
    fn a_cover_position_goes_with_its_command_only() {
        assert_eq!(
            cover_command(CoverOrder::Open, None),
            Ok(CoverCommand::Open)
        );
        assert_eq!(
            cover_command(CoverOrder::Close, None),
            Ok(CoverCommand::Close)
        );
        assert_eq!(
            cover_command(CoverOrder::Stop, None),
            Ok(CoverCommand::Stop)
        );
        assert_eq!(
            cover_command(CoverOrder::Position, Some(0)),
            Ok(CoverCommand::OpenPercent(0))
        );
        assert_eq!(
            cover_command(CoverOrder::Position, Some(100)),
            Ok(CoverCommand::OpenPercent(100))
        );
        assert!(cover_command(CoverOrder::Position, Some(101)).is_err());
        assert!(cover_command(CoverOrder::Position, None).is_err());
        assert!(cover_command(CoverOrder::Close, Some(30)).is_err());
        let wrong = |position| {
            vec![Action::Cover {
                cover: "1".into(),
                command: CoverOrder::Position,
                position,
            }]
        };
        assert!(validate(&wrong(Some(101))).is_err(), "refused at save");
        assert!(validate(&wrong(None)).is_err());
        let hue = |brightness| {
            vec![Action::HueBrightness {
                lamp: "aa".into(),
                brightness,
            }]
        };
        assert!(validate(&hue(0)).is_err() && validate(&hue(101)).is_err());
        assert!(validate(&hue(1)).is_ok() && validate(&hue(100)).is_ok());
    }

    /// The AC comes back as it was last set, but a sleep timer armed then is not armed again.
    #[test]
    fn climate_on_resumes_the_last_settings_without_their_timer() {
        let last =
            mitsubishi_ir::parse_climate_settings("state-heat-21-fan-auto-vane-auto-stopin-60")
                .unwrap();
        let again = resumed(Some(last.clone())).expect("settings");
        assert_eq!(
            again,
            ClimateSettings {
                stop_in_minutes: None,
                ..last
            }
        );
        assert_eq!(
            again.command().unwrap(),
            "state-heat-21-fan-auto-vane-auto-wide-center"
        );
        assert!(
            matches!(
                resumed(None),
                Err(AppError::Coded {
                    code: "not_configured",
                    ..
                })
            ),
            "nothing to resume: said so"
        );
    }

    /// The codes a failed action is said with: its own refusal's, or the nearest by kind.
    #[test]
    fn a_failure_is_said_by_its_code() {
        use axum::http::StatusCode;
        assert_eq!(
            code_of(&Refusal::TooManyActions.into()),
            Refusal::TooManyActions
        );
        assert_eq!(
            code_of(&AppError::http(StatusCode::NOT_FOUND, "x")),
            Refusal::NotFound
        );
        assert_eq!(
            code_of(&AppError::service_unavailable("TV unreachable")),
            Refusal::DeviceUnreachable
        );
        assert_eq!(
            code_of(&AppError::http(StatusCode::BAD_REQUEST, "x")),
            Refusal::Invalid
        );
        assert_eq!(
            code_of(&AppError::Io(std::io::Error::other("disk"))),
            Refusal::Internal
        );
        let off = Action::MerossPower {
            device: "p1".into(),
            state: SwitchState::Off,
        };
        assert_eq!(
            result(&off, Err(AppError::service_unavailable("Plug unreachable"))),
            ActionResult {
                action: off.clone(),
                ok: false,
                code: Some("device_unreachable".into())
            }
        );
        assert!(result(&off, Ok(())).ok);
    }

    /// Brightness 0 is off, for both lamp families: an action says off with `*_power`.
    #[test]
    fn a_brightness_is_one_to_a_hundred_for_every_lamp() {
        for brightness in [0, 101] {
            for action in [
                Action::HueBrightness {
                    lamp: "aa".into(),
                    brightness,
                },
                Action::ZigbeeBrightness {
                    lamp: "aa".into(),
                    brightness,
                },
            ] {
                assert!(
                    validate(std::slice::from_ref(&action)).is_err(),
                    "{action:?}"
                );
            }
        }
        assert!(
            validate(&[Action::ZigbeeBrightness {
                lamp: "aa".into(),
                brightness: 1
            }])
            .is_ok()
        );
    }

    #[tokio::test]
    async fn resolve_asks_the_current_state_only_to_toggle() {
        let never = || async { panic!("not asked for a forced state") };
        assert!(SwitchState::On.resolve(never).await.unwrap());
        assert!(!SwitchState::Off.resolve(never).await.unwrap());
        assert!(
            !SwitchState::Toggle
                .resolve(|| async { Ok(true) })
                .await
                .unwrap()
        );
        assert!(
            SwitchState::Toggle
                .resolve(|| async { Ok(false) })
                .await
                .unwrap()
        );
        assert!(
            SwitchState::Toggle
                .resolve(|| async { Err(Refusal::NotFound.into()) })
                .await
                .is_err()
        );
    }
}
