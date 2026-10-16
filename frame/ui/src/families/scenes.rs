//! « Scènes » near the top of the dashboard (ScenesGroup, scenes/scenes.ts): one button per
//! scene, each run said once (every action done, or which failed and what they said), then the
//! families it may have changed read again. Making and editing scenes stays in the web app;
//! with none, the group is not shown (the web app's « Créer » is an admin's).

use maison_api::remote::KeyResponse;
use maison_api::scenes::Scene;

use super::{Cx, HUE, MEROSS, SCENES, SHUTTERS, ZIGBEE};
use crate::commands::{Order, Outcome, Post, Toast, limit};
use crate::i18n::{Arg, Words, k};
use crate::paths;
use crate::ui::icon::{self, Icon};
use crate::ui::model::{Act, Body, Button, Group};

/// The icon a scene is drawn with (`maison_api::scenes::icon_of`: one of `ICONS`, else the
/// sparkles), as the web app's.
pub fn icon_of(s: &Scene) -> Icon {
    Icon::named(maison_api::scenes::icon_of(s)).unwrap_or(icon::SPARKLES)
}

/// How a run went (scenes.ts `sayRun`).
pub fn said(w: &Words, name: &str, r: &KeyResponse) -> Toast {
    let failed: Vec<&str> = r.outcomes().filter(|(ok, _)| !ok).map(|(_, d)| d).collect();
    if failed.is_empty() {
        return Toast {
            text: w.t(
                k::SCENES_DONE,
                &[
                    ("name", name.into()),
                    ("count", Arg::Count(r.results.len() as f64)),
                ],
            ),
            warn: false,
        };
    }
    Toast {
        text: w.t(
            k::SCENES_FAILED,
            &[
                ("name", name.into()),
                ("count", Arg::Count(failed.len() as f64)),
                ("total", Arg::Count(r.results.len() as f64)),
                ("details", failed.join(" · ").into()),
            ],
        ),
        warn: true,
    }
}

fn run_said(w: &Words, o: &Outcome) -> Option<Toast> {
    let name = o.done.first().or(o.failed.first())?;
    match o
        .answer
        .as_ref()
        .map(|v| serde_json::from_value::<KeyResponse>(v.clone()))
    {
        Some(Ok(r)) => Some(said(w, name, &r)),
        // refused, unreachable, or an answer that does not read: « Pas de réponse » for it
        _ => Some(Toast {
            text: w.t(k::COMMAND_NO_ANSWER, &[("name", name.into())]),
            warn: true,
        }),
    }
}

pub fn button(s: &Scene) -> Button {
    let order = Order::one(
        format!("{SCENES}:{}:run", s.id),
        s.name.clone(),
        Post::empty(paths::of(paths::SCENES, &s.id, "run")),
    )
    .within(limit::DEVICE)
    // what a scene may have changed (the web app reads these again)
    .then_refresh(&[HUE, ZIGBEE, MEROSS, SHUTTERS])
    .said(run_said);
    Button::new(Some(icon_of(s)), s.name.clone(), Act::Order(order))
}

pub fn scenes_group(w: &Words, scenes: &[Scene]) -> Option<Group> {
    if scenes.is_empty() {
        return None;
    }
    Some(Group {
        title: w.s(k::SCENES_TITLE),
        fact: None,
        actions: Vec::new(),
        stale: None,
        body: Body::Buttons(scenes.iter().map(button).collect()),
    })
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let v = cx.live.want(&cx.sources.scenes);
    scenes_group(
        cx.words,
        v.data
            .as_deref()
            .map(|d| d.scenes.as_slice())
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use maison_api::Answer;
    use maison_api::scenes::SceneList;

    #[test]
    fn a_button_per_scene_and_its_run_said() {
        let w = Words::new(Lang::Fr);
        let list: SceneList = serde_json::from_str::<Answer<SceneList>>(
            r#"{"success":true,"scenes":[{"id":"nuit","name":"Nuit","icon":"moon","actions":[]},{"id":"x","name":"X","icon":"rocket","actions":[]}]}"#,
        )
        .unwrap()
        .body;
        let g = scenes_group(&w, &list.scenes).unwrap();
        let Body::Buttons(b) = &g.body else { panic!() };
        assert_eq!(b[0].icon, Some(icon::MOON));
        assert_eq!(
            b[1].icon,
            Some(icon::SPARKLES),
            "an icon the app does not know"
        );
        let Act::Order(o) = &b[0].act else { panic!() };
        assert_eq!(o.posts[0].1.path, "/scenes/nuit/run");
        assert!(scenes_group(&w, &[]).is_none());

        let ok = said(
            &w,
            "Nuit",
            &KeyResponse::ran(vec!["ok: a".into(), "ok: b".into()]),
        );
        assert_eq!(
            ok,
            Toast {
                text: "Nuit\u{a0}: 2 actions faites".into(),
                warn: false
            }
        );
        let partial = said(
            &w,
            "Nuit",
            &KeyResponse::ran(vec!["ok: a".into(), "failed: TV injoignable".into()]),
        );
        assert!(partial.warn);
        assert_eq!(
            partial.text,
            "Nuit\u{a0}: 1 action sur 2 a échoué · TV injoignable"
        );
    }
}
