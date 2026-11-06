use hp_vmf::Connection;

use crate::document::{Document, is_reserved_key};
use crate::glob::glob_match;
use crate::{EditError, EditResult, Id};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityQuery {
    pub classname: Option<String>,
    pub targetname: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub include_hidden: bool,
}

impl Document {
    pub fn find_entities(&self, q: &EntityQuery) -> Vec<Id> {
        self.scene
            .entities
            .values()
            .filter(|n| {
                let e = &n.entity;
                if !q.include_hidden && !self.is_entity_visible(e.id) {
                    return false;
                }
                if let Some(c) = &q.classname
                    && !glob_match(c, &e.classname)
                {
                    return false;
                }
                if let Some(t) = &q.targetname
                    && !e.get("targetname").is_some_and(|v| glob_match(t, v))
                {
                    return false;
                }
                match (&q.key, &q.value) {
                    (None, None) => true,
                    (Some(k), None) => e.props.iter().any(|(pk, _)| glob_match(k, pk)),
                    (None, Some(v)) => e.props.iter().any(|(_, pv)| glob_match(v, pv)),
                    (Some(k), Some(v)) => e.props.iter().any(|(pk, pv)| glob_match(k, pk) && glob_match(v, pv)),
                }
            })
            .map(|n| n.entity.id)
            .collect()
    }

    pub fn entities_named(&self, name: &str) -> Vec<Id> {
        self.scene
            .entities
            .values()
            .filter(|n| n.entity.get("targetname").is_some_and(|t| t.eq_ignore_ascii_case(name)))
            .map(|n| n.entity.id)
            .collect()
    }

    pub fn outputs_targeting(&self, name: &str) -> Vec<(Id, usize)> {
        let mut out = Vec::new();
        let world = &self.scene.world;
        for (i, c) in world.connections.iter().enumerate() {
            if c.target.eq_ignore_ascii_case(name) {
                out.push((world.id, i));
            }
        }
        for n in self.scene.entities.values() {
            for (i, c) in n.entity.connections.iter().enumerate() {
                if c.target.eq_ignore_ascii_case(name) {
                    out.push((n.entity.id, i));
                }
            }
        }
        out
    }

    pub(crate) fn check_entities(&self, entities: &[Id]) -> EditResult<()> {
        for id in entities {
            if !self.scene.entities.contains_key(id) {
                return Err(EditError::NotFound(crate::ObjRef::Entity(*id)));
            }
        }
        Ok(())
    }

    pub fn edit_entity(&mut self, id: Id, f: impl FnOnce(&mut hp_vmf::Entity)) -> EditResult<bool> {
        let node = self.scene.entities.get(&id).ok_or(EditError::NotFound(crate::ObjRef::Entity(id)))?;
        let mut e = node.entity.clone();
        f(&mut e);
        e.id = id;
        e.solids.clear();
        if e.classname.trim().is_empty() {
            return Err(EditError::Invalid("empty class name".to_owned()));
        }
        e.props.retain(|(k, _)| !is_reserved_key(k));
        if e == node.entity {
            return Ok(false);
        }
        self.tx("Edit entity", |doc| {
            doc.update_entity(id, |x| *x = e);
            Ok(true)
        })
    }

    pub fn edit_solid(&mut self, id: Id, f: impl FnOnce(&mut hp_vmf::Solid)) -> EditResult<bool> {
        let node = self.scene.solids.get(&id).ok_or(EditError::NotFound(crate::ObjRef::Solid(id)))?;
        let mut s = node.solid.clone();
        f(&mut s);
        s.id = id;
        if s == node.solid {
            return Ok(false);
        }
        hp_geom::validate_solid(&s.sides)?;
        self.tx("Edit solid", |doc| {
            doc.update_solid(id, s);
            doc.deselect_missing();
            Ok(true)
        })
    }

    pub fn set_entity_key(&mut self, entities: &[Id], key: &str, value: &str) -> EditResult<usize> {
        let key = key.trim();
        if key.eq_ignore_ascii_case("classname") {
            return self.set_entity_class(entities, value);
        }
        if key.is_empty() || is_reserved_key(key) {
            return Err(EditError::Invalid(format!("key '{key}' cannot be set")));
        }
        self.check_entities(entities)?;
        let targets: Vec<Id> = entities
            .iter()
            .copied()
            .filter(|id| self.scene.entities.get(id).is_some_and(|n| n.entity.get(key) != Some(value)))
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        self.tx("Edit entity", |doc| {
            for id in &targets {
                doc.update_entity(*id, |e| e.set(key, value));
            }
            Ok(targets.len())
        })
    }

    pub fn remove_entity_key(&mut self, entities: &[Id], key: &str) -> EditResult<usize> {
        self.check_entities(entities)?;
        let targets: Vec<Id> = entities
            .iter()
            .copied()
            .filter(|id| self.scene.entities.get(id).is_some_and(|n| n.entity.get(key).is_some()))
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        self.tx("Edit entity", |doc| {
            for id in &targets {
                doc.update_entity(*id, |e| while e.remove(key).is_some() {});
            }
            Ok(targets.len())
        })
    }

    pub fn rename_entity_key(&mut self, entities: &[Id], old: &str, new: &str) -> EditResult<usize> {
        let new = new.trim();
        if new.is_empty() || is_reserved_key(new) {
            return Err(EditError::Invalid(format!("key '{new}' cannot be used")));
        }
        self.check_entities(entities)?;
        if old.eq_ignore_ascii_case(new) && old == new {
            return Ok(0);
        }
        let targets: Vec<Id> = entities
            .iter()
            .copied()
            .filter(|id| self.scene.entities.get(id).is_some_and(|n| n.entity.get(old).is_some()))
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        self.tx("Edit entity", |doc| {
            for id in &targets {
                doc.update_entity(*id, |e| {
                    let Some(pos) = e.props.iter().position(|(k, _)| k.eq_ignore_ascii_case(old)) else { return };
                    let value = e.props.remove(pos).1;
                    e.props.retain(|(k, _)| !k.eq_ignore_ascii_case(new));
                    let at = pos.min(e.props.len());
                    e.props.insert(at, (new.to_owned(), value));
                });
            }
            Ok(targets.len())
        })
    }

    pub fn set_entity_class(&mut self, entities: &[Id], classname: &str) -> EditResult<usize> {
        let classname = classname.trim();
        if classname.is_empty() {
            return Err(EditError::Invalid("empty class name".to_owned()));
        }
        self.check_entities(entities)?;
        let targets: Vec<Id> = entities
            .iter()
            .copied()
            .filter(|id| {
                self.scene.entities.get(id).is_some_and(|n| {
                    n.entity.classname != classname
                        || (hp_vmf::is_world_particle_volume(classname)
                            && self.scene.entity_solid_count(*id) > 0
                            && n.entity.get("origin").is_some())
                })
            })
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        self.tx("Change class", |doc| {
            for id in &targets {
                let world_particles =
                    hp_vmf::is_world_particle_volume(classname) && doc.scene.entity_solid_count(*id) > 0;
                doc.update_entity(*id, |e| {
                    classname.clone_into(&mut e.classname);
                    if world_particles {
                        while e.remove("origin").is_some() {}
                    }
                });
            }
            Ok(targets.len())
        })
    }

    pub fn add_connection(&mut self, entities: &[Id], connection: &Connection) -> EditResult<usize> {
        if connection.output.trim().is_empty() {
            return Err(EditError::Invalid("empty output name".to_owned()));
        }
        self.check_entities(entities)?;
        if entities.is_empty() {
            return Ok(0);
        }
        self.tx("Add output", |doc| {
            for id in entities {
                doc.update_entity(*id, |e| e.connections.push(connection.clone()));
            }
            Ok(entities.len())
        })
    }

    pub fn set_connection(&mut self, entity: Id, index: usize, connection: Connection) -> EditResult<()> {
        let n = self.scene.entities.get(&entity).ok_or(EditError::NotFound(crate::ObjRef::Entity(entity)))?;
        let current =
            n.entity.connections.get(index).ok_or_else(|| EditError::Invalid(format!("no output {index}")))?;
        if *current == connection {
            return Ok(());
        }
        self.tx("Edit output", |doc| {
            doc.update_entity(entity, |e| {
                if let Some(slot) = e.connections.get_mut(index) {
                    *slot = connection;
                }
            });
            Ok(())
        })
    }

    pub fn remove_connection(&mut self, entity: Id, index: usize) -> EditResult<()> {
        let n = self.scene.entities.get(&entity).ok_or(EditError::NotFound(crate::ObjRef::Entity(entity)))?;
        if index >= n.entity.connections.len() {
            return Err(EditError::Invalid(format!("no output {index}")));
        }
        self.tx("Delete output", |doc| {
            doc.update_entity(entity, |e| {
                if index < e.connections.len() {
                    e.connections.remove(index);
                }
            });
            Ok(())
        })
    }

    pub fn set_connections(&mut self, entity: Id, connections: Vec<Connection>) -> EditResult<()> {
        let n = self.scene.entities.get(&entity).ok_or(EditError::NotFound(crate::ObjRef::Entity(entity)))?;
        if n.entity.connections == connections {
            return Ok(());
        }
        self.tx("Edit outputs", |doc| {
            doc.update_entity(entity, |e| e.connections = connections);
            Ok(())
        })
    }

    pub fn remove_connections_where(
        &mut self,
        entities: &[Id],
        pred: &dyn Fn(&Connection) -> bool,
    ) -> EditResult<usize> {
        self.check_entities(entities)?;
        let mut total = 0;
        let mut targets = Vec::new();
        for id in entities {
            if let Some(n) = self.scene.entities.get(id) {
                let k = n.entity.connections.iter().filter(|c| pred(c)).count();
                if k > 0 {
                    total += k;
                    targets.push(*id);
                }
            }
        }
        if targets.is_empty() {
            return Ok(0);
        }
        self.tx("Delete output", |doc| {
            for id in targets {
                doc.update_entity(id, |e| e.connections.retain(|c| !pred(c)));
            }
            Ok(total)
        })
    }

    pub fn rename_targetname(&mut self, old: &str, new: &str, update_references: bool) -> EditResult<usize> {
        let new = new.trim();
        if old.trim().is_empty() || new.is_empty() {
            return Err(EditError::Invalid("empty entity name".to_owned()));
        }
        let named = self.entities_named(old);
        let refs = if update_references { self.outputs_targeting(old) } else { Vec::new() };
        if named.is_empty() && refs.is_empty() {
            return Ok(0);
        }
        let world_id = self.scene.world.id;
        self.tx("Rename entity", |doc| {
            for id in &named {
                doc.update_entity(*id, |e| e.set("targetname", new));
            }
            for (id, index) in refs {
                let fix = |c: &mut Connection| {
                    if c.target.eq_ignore_ascii_case(old) {
                        new.clone_into(&mut c.target);
                    }
                };
                if id == world_id && !doc.scene.entities.contains_key(&id) {
                    if let Some(c) = doc.world_mut().connections.get_mut(index) {
                        fix(c);
                    }
                } else {
                    doc.update_entity(id, |e| {
                        if let Some(c) = e.connections.get_mut(index) {
                            fix(c);
                        }
                    });
                }
            }
            Ok(named.len())
        })
    }
}
