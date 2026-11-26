use crate::audio::{SfxEngine, SfxKind};
use crate::camera::UiInputCapture;
use crate::desks::DeskSlot;
use crate::palette::{WorldEntity, WorldPalette};
use crate::people::LiveSessionWorker;
use bevy::picking::Pickable;
use bevy::picking::events::{Click, Out, Over, Pointer};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Component, Debug, Clone)]
pub struct InteractiveProp {
    pub id: &'static str,
    pub place: &'static str,
    pub label: &'static str,
}

impl InteractiveProp {
    pub const fn new(id: &'static str, place: &'static str, label: &'static str) -> Self {
        Self { id, place, label }
    }
}

#[derive(Event, Debug, Clone)]
pub struct PropActivated {
    pub id: &'static str,
    pub place: &'static str,
    pub label: &'static str,
}

#[derive(Event, Debug, Clone)]
pub struct WorkerSelected {
    pub session_id: String,
    pub agent: String,
    pub name: String,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct PlaceCardState {
    pub active_place: Option<String>,
    pub active_prop_label: Option<String>,
    pub hovered_label: Option<String>,
    pub selected_session_id: Option<String>,
    hovered_root: Option<Entity>,
    hovered_parts: HashSet<Entity>,
}

#[derive(Component)]
pub struct HoverRing;

pub fn spawn_hover_ring(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
) {
    let ring_m = palette.emissive(materials, "#FFB23F", 1.4);
    commands.spawn((
        WorldEntity,
        HoverRing,
        Pickable::IGNORE,
        Mesh3d(palette.unit_torus.clone()),
        MeshMaterial3d(ring_m),
        Transform::from_xyz(0.0, -50.0, 0.0).with_scale(Vec3::new(0.8, 0.15, 0.8)),
        Visibility::Hidden,
    ));
}

#[derive(Clone)]
enum PickTarget {
    Prop {
        id: &'static str,
        place: &'static str,
        label: &'static str,
        pos: Vec3,
    },
    Worker {
        session_id: String,
        agent: String,
        name: String,
        pos: Vec3,
    },
    Desk {
        index: usize,
        pos: Vec3,
    },
}

fn entity_pos(
    ent: Entity,
    g_q: &Query<&GlobalTransform>,
    t_q: &Query<&Transform, Without<HoverRing>>,
) -> Vec3 {
    if let Ok(gt) = g_q.get(ent) {
        gt.translation()
    } else if let Ok(tf) = t_q.get(ent) {
        tf.translation
    } else {
        Vec3::ZERO
    }
}

fn resolve_pick_target(
    start: Entity,
    parents_q: &Query<&ChildOf>,
    props_q: &Query<&InteractiveProp>,
    workers_q: &Query<&LiveSessionWorker>,
    desks_q: &Query<&DeskSlot>,
    g_q: &Query<&GlobalTransform>,
    t_q: &Query<&Transform, Without<HoverRing>>,
) -> Option<(Entity, PickTarget)> {
    let mut cur = start;
    for _ in 0..12 {
        if let Ok(w) = workers_q.get(cur) {
            return Some((
                cur,
                PickTarget::Worker {
                    session_id: w.session_id.clone(),
                    agent: w.agent.clone(),
                    name: w.name.clone(),
                    pos: entity_pos(cur, g_q, t_q),
                },
            ));
        }
        if let Ok(p) = props_q.get(cur) {
            return Some((
                cur,
                PickTarget::Prop {
                    id: p.id,
                    place: p.place,
                    label: p.label,
                    pos: entity_pos(cur, g_q, t_q),
                },
            ));
        }
        if let Ok(d) = desks_q.get(cur) {
            return Some((
                cur,
                PickTarget::Desk {
                    index: d.index,
                    pos: entity_pos(cur, g_q, t_q),
                },
            ));
        }
        let Ok(parent) = parents_q.get(cur) else {
            break;
        };
        cur = parent.parent();
    }
    None
}

fn sfx_for_prop(id: &str) -> SfxKind {
    match id {
        "cat" => SfxKind::Meow,
        "arcade" => SfxKind::Coin,
        "printer" | "shredder" => SfxKind::Print,
        "billiards" => SfxKind::Pock,
        "aquarium" => SfxKind::Bloop,
        "karaoke" => SfxKind::Bell,
        "microwave" => SfxKind::Micro,
        "vacuum" => SfxKind::Pop,
        "whiteboard" => SfxKind::Rustle,
        _ => SfxKind::Click,
    }
}

#[allow(clippy::too_many_arguments)]
fn on_pointer_over(
    mut trigger: Trigger<Pointer<Over>>,
    ui_capture: Option<Res<UiInputCapture>>,
    mut place_card: Option<ResMut<PlaceCardState>>,
    mut ring_q: Query<(&mut Transform, &mut Visibility), With<HoverRing>>,
    parents_q: Query<&ChildOf>,
    props_q: Query<&InteractiveProp>,
    workers_q: Query<&LiveSessionWorker>,
    desks_q: Query<&DeskSlot>,
    g_q: Query<&GlobalTransform>,
    t_q: Query<&Transform, Without<HoverRing>>,
) {
    if ui_capture.as_ref().is_some_and(|c| c.wants_pointer) {
        return;
    }
    let hit_ent = trigger.target();
    let Some((root, target)) = resolve_pick_target(
        hit_ent,
        &parents_q,
        &props_q,
        &workers_q,
        &desks_q,
        &g_q,
        &t_q,
    ) else {
        return;
    };
    trigger.propagate(false);
    let (pos, label) = match target {
        PickTarget::Prop { label, pos, .. } => (pos, label.to_owned()),
        PickTarget::Worker { agent, name, pos, .. } => {
            (pos, format!("{} · {}", agent.to_uppercase(), name))
        }
        PickTarget::Desk { index, pos } => (pos, format!("Desk #{}", index + 1)),
    };
    for (mut tf, mut vis) in &mut ring_q {
        tf.translation = Vec3::new(pos.x, 0.04, pos.z);
        *vis = Visibility::Visible;
    }
    if let Some(pc) = place_card.as_deref_mut() {
        if pc.hovered_root != Some(root) {
            pc.hovered_parts.clear();
            pc.hovered_root = Some(root);
        }
        pc.hovered_parts.insert(hit_ent);
        pc.hovered_label = Some(label);
    }
}

fn on_pointer_out(
    mut trigger: Trigger<Pointer<Out>>,
    mut place_card: Option<ResMut<PlaceCardState>>,
    mut ring_q: Query<&mut Visibility, With<HoverRing>>,
    parents_q: Query<&ChildOf>,
    props_q: Query<&InteractiveProp>,
    workers_q: Query<&LiveSessionWorker>,
    desks_q: Query<&DeskSlot>,
    g_q: Query<&GlobalTransform>,
    t_q: Query<&Transform, Without<HoverRing>>,
) {
    let hit_ent = trigger.target();
    let Some((root, _)) = resolve_pick_target(
        hit_ent,
        &parents_q,
        &props_q,
        &workers_q,
        &desks_q,
        &g_q,
        &t_q,
    ) else {
        return;
    };
    trigger.propagate(false);
    if let Some(pc) = place_card.as_deref_mut() {
        if pc.hovered_root != Some(root) {
            return;
        }
        pc.hovered_parts.remove(&hit_ent);
        if !pc.hovered_parts.is_empty() {
            // Pointer moved to a sibling child mesh of the same interactive root: keep hover ring visible.
            return;
        }
        pc.hovered_root = None;
        pc.hovered_label = None;
    }
    for mut vis in &mut ring_q {
        *vis = Visibility::Hidden;
    }
}

#[allow(clippy::too_many_arguments)]
fn on_pointer_click(
    mut trigger: Trigger<Pointer<Click>>,
    ui_capture: Option<Res<UiInputCapture>>,
    mut place_card: Option<ResMut<PlaceCardState>>,
    mut sfx: Option<ResMut<SfxEngine>>,
    mut prop_events: EventWriter<PropActivated>,
    mut worker_events: EventWriter<WorkerSelected>,
    parents_q: Query<&ChildOf>,
    props_q: Query<&InteractiveProp>,
    workers_q: Query<&LiveSessionWorker>,
    desks_q: Query<&DeskSlot>,
    g_q: Query<&GlobalTransform>,
    t_q: Query<&Transform, Without<HoverRing>>,
) {
    if ui_capture.as_ref().is_some_and(|c| c.wants_pointer) {
        return;
    }
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let Some((_, target)) = resolve_pick_target(
        trigger.target(),
        &parents_q,
        &props_q,
        &workers_q,
        &desks_q,
        &g_q,
        &t_q,
    ) else {
        return;
    };
    trigger.propagate(false);

    match target {
        PickTarget::Prop {
            id, place, label, ..
        } => {
            if let Some(pc) = place_card.as_deref_mut() {
                pc.active_place = Some(place.to_owned());
                pc.active_prop_label = Some(label.to_owned());
            }
            prop_events.write(PropActivated { id, place, label });
            if let Some(eng) = sfx.as_deref_mut() {
                eng.play_sfx(sfx_for_prop(id));
            }
        }
        PickTarget::Worker {
            session_id,
            agent,
            name,
            ..
        } => {
            if let Some(pc) = place_card.as_deref_mut() {
                pc.active_place = Some("desk".to_owned());
                pc.active_prop_label = Some(format!("{} · {}", agent.to_uppercase(), name));
                pc.selected_session_id = Some(session_id.clone());
            }
            worker_events.write(WorkerSelected {
                session_id,
                agent,
                name,
            });
            if let Some(eng) = sfx.as_deref_mut() {
                eng.play_sfx(SfxKind::Pop);
            }
        }
        PickTarget::Desk { index, .. } => {
            if let Some(w) = workers_q.iter().find(|w| w.desk_index == index) {
                if let Some(pc) = place_card.as_deref_mut() {
                    pc.active_place = Some("desk".to_owned());
                    pc.active_prop_label = Some(format!("{} · {}", w.agent.to_uppercase(), w.name));
                    pc.selected_session_id = Some(w.session_id.clone());
                }
                worker_events.write(WorkerSelected {
                    session_id: w.session_id.clone(),
                    agent: w.agent.clone(),
                    name: w.name.clone(),
                });
            } else if let Some(pc) = place_card.as_deref_mut() {
                pc.active_place = Some("desk".to_owned());
                pc.active_prop_label = Some(format!("Desk #{}", index + 1));
            }
            if let Some(eng) = sfx.as_deref_mut() {
                eng.play_sfx(SfxKind::Click);
            }
        }
    }
}

/// Registers global `bevy_picking` pointer observers (`Over`, `Out`, `Click`) for interactive
/// desks, dynamic session workers, and office props.
pub fn register_picking_observers(app: &mut App) {
    app.add_observer(on_pointer_over)
        .add_observer(on_pointer_out)
        .add_observer(on_pointer_click);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SfxEngine;
    use crate::camera::UiInputCapture;
    use crate::people::LiveSessionWorker;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Click, Out, Over, Pointer};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    use bevy::render::camera::NormalizedRenderTarget;

    fn dummy_location() -> Location {
        Location {
            target: NormalizedRenderTarget::Image(Handle::default().into()),
            position: Vec2::ZERO,
        }
    }

    #[test]
    fn picking_observers_move_hover_ring_and_dispatch_prop_and_worker_selection() {
        let mut app = App::new();
        app.insert_resource(PlaceCardState::default())
            .insert_resource(UiInputCapture::default())
            .insert_resource(SfxEngine::new(false))
            .add_event::<PropActivated>()
            .add_event::<WorkerSelected>();
        register_picking_observers(&mut app);

        let ring = app
            .world_mut()
            .spawn((
                HoverRing,
                Transform::from_xyz(0.0, -50.0, 0.0),
                Visibility::Hidden,
            ))
            .id();

        let prop_ent = app
            .world_mut()
            .spawn((
                InteractiveProp::new("cat", "lounge", "Office Cat"),
                Transform::from_xyz(3.0, 0.2, 4.0),
                GlobalTransform::from_xyz(3.0, 0.2, 4.0),
            ))
            .id();

        let worker_ent = app
            .world_mut()
            .spawn((
                LiveSessionWorker {
                    session_id: "sess-pick-1".to_owned(),
                    agent: "claude".to_owned(),
                    name: "agents-station".to_owned(),
                    desk_index: 0,
                    status: "work".to_owned(),
                    tool_label: Some("Edit".to_owned()),
                    git_dirty: 0,
                },
                Transform::from_xyz(1.5, 0.0, 2.5),
                GlobalTransform::from_xyz(1.5, 0.0, 2.5),
            ))
            .id();

        let hit = HitData::new(Entity::PLACEHOLDER, 1.0, None, None);

        // Hover over prop -> HoverRing becomes Visible at (3.0, 0.04, 4.0)
        app.world_mut().trigger_targets(
            Pointer::new(PointerId::Mouse, dummy_location(), prop_ent, Over { hit: hit.clone() }),
            prop_ent,
        );
        app.update();
        assert_eq!(*app.world().get::<Visibility>(ring).unwrap(), Visibility::Visible);
        let ring_pos = app.world().get::<Transform>(ring).unwrap().translation;
        assert!((ring_pos.x - 3.0).abs() < 1e-4 && (ring_pos.z - 4.0).abs() < 1e-4);

        // Click prop -> PlaceCardState updated + PropActivated emitted
        app.world_mut().trigger_targets(
            Pointer::new(
                PointerId::Mouse,
                dummy_location(),
                prop_ent,
                Click {
                    button: PointerButton::Primary,
                    hit: hit.clone(),
                    duration: std::time::Duration::from_millis(50),
                },
            ),
            prop_ent,
        );
        app.update();
        assert_eq!(
            app.world().resource::<PlaceCardState>().active_place.as_deref(),
            Some("lounge")
        );
        assert_eq!(
            app.world().resource::<PlaceCardState>().active_prop_label.as_deref(),
            Some("Office Cat")
        );
        let prop_events = app.world().resource::<Events<PropActivated>>();
        assert_eq!(prop_events.len(), 1);

        // Click worker -> PlaceCardState.selected_session_id set + WorkerSelected emitted
        app.world_mut().trigger_targets(
            Pointer::new(
                PointerId::Mouse,
                dummy_location(),
                worker_ent,
                Click {
                    button: PointerButton::Primary,
                    hit: hit.clone(),
                    duration: std::time::Duration::from_millis(50),
                },
            ),
            worker_ent,
        );
        app.update();
        assert_eq!(
            app.world().resource::<PlaceCardState>().selected_session_id.as_deref(),
            Some("sess-pick-1")
        );
        let worker_events = app.world().resource::<Events<WorkerSelected>>();
        assert_eq!(worker_events.len(), 1);

        // Pointer out -> HoverRing hides
        app.world_mut().trigger_targets(
            Pointer::new(PointerId::Mouse, dummy_location(), prop_ent, Out { hit }),
            prop_ent,
        );
        app.update();
        assert_eq!(*app.world().get::<Visibility>(ring).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn picking_child_meshes_dedupes_click_and_keeps_hover_across_sibling_children() {
        let mut app = App::new();
        app.insert_resource(PlaceCardState::default())
            .insert_resource(UiInputCapture::default())
            .insert_resource(SfxEngine::new(false))
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .add_event::<PropActivated>()
            .add_event::<WorkerSelected>();
        register_picking_observers(&mut app);

        // Spawn HoverRing via spawn_hover_ring and verify Pickable::IGNORE is attached.
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                let mut pal = WorldPalette::new(&mut meshes);
                let mut cmds = world.commands();
                spawn_hover_ring(&mut cmds, &mut pal, &mut mats);
            });
        });
        app.world_mut().flush();
        let (ring, pickable) = app
            .world_mut()
            .query_filtered::<(Entity, &Pickable), With<HoverRing>>()
            .single(app.world())
            .expect("HoverRing must exist with Pickable");
        assert_eq!(*pickable, Pickable::IGNORE, "HoverRing must not self-pick");

        let worker_root = app
            .world_mut()
            .spawn((
                LiveSessionWorker {
                    session_id: "sess-child-test".to_owned(),
                    agent: "claude".to_owned(),
                    name: "agents-station".to_owned(),
                    desk_index: 0,
                    status: "work".to_owned(),
                    tool_label: Some("Edit".to_owned()),
                    git_dirty: 0,
                },
                Transform::from_xyz(2.0, 0.0, 3.0),
                GlobalTransform::from_xyz(2.0, 0.0, 3.0),
            ))
            .id();
        let child_a = app.world_mut().spawn(Mesh3d(Handle::default())).id();
        let child_b = app.world_mut().spawn(Mesh3d(Handle::default())).id();
        app.world_mut()
            .entity_mut(worker_root)
            .add_children(&[child_a, child_b]);
        app.world_mut().flush();

        let hit = HitData::new(Entity::PLACEHOLDER, 1.0, None, None);

        // Clicking child_a must NOT propagate to worker_root and duplicate WorkerSelected.
        app.world_mut().trigger_targets(
            Pointer::new(
                PointerId::Mouse,
                dummy_location(),
                child_a,
                Click {
                    button: PointerButton::Primary,
                    hit: hit.clone(),
                    duration: std::time::Duration::from_millis(40),
                },
            ),
            child_a,
        );
        app.update();
        assert_eq!(
            app.world().resource::<Events<WorkerSelected>>().len(),
            1,
            "clicking a child mesh must emit exactly ONE WorkerSelected event"
        );

        // Hover child_a -> hover child_b -> out of child_a must keep HoverRing visible & label intact.
        app.world_mut().trigger_targets(
            Pointer::new(PointerId::Mouse, dummy_location(), child_a, Over { hit: hit.clone() }),
            child_a,
        );
        app.world_mut().trigger_targets(
            Pointer::new(PointerId::Mouse, dummy_location(), child_b, Over { hit: hit.clone() }),
            child_b,
        );
        app.world_mut().trigger_targets(
            Pointer::new(PointerId::Mouse, dummy_location(), child_a, Out { hit: hit.clone() }),
            child_a,
        );
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(ring).unwrap(),
            Visibility::Visible,
            "HoverRing must remain Visible when moving between sibling child meshes"
        );
        assert!(
            app.world().resource::<PlaceCardState>().hovered_label.is_some(),
            "hovered_label must remain set while sibling child_b is still hovered"
        );

        // Out of child_b clears hover completely.
        app.world_mut().trigger_targets(
            Pointer::new(PointerId::Mouse, dummy_location(), child_b, Out { hit }),
            child_b,
        );
        app.update();
        assert_eq!(*app.world().get::<Visibility>(ring).unwrap(), Visibility::Hidden);
        assert!(app.world().resource::<PlaceCardState>().hovered_label.is_none());
    }
}
