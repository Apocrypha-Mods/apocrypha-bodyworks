//! What every object in the scene starts with: a name, a transform, and its
//! links to other blocks. And the node, which holds children.

use crate::io::{Reader, Writer};
use crate::NifError;

/// A link to another block, by index; -1 for none.
pub type Ref = i32;

/// Where something is, relative to its parent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Row-major 3 × 3.
    pub rotation: [f32; 9],
    pub scale: f32,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        translation: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
    };

    pub(crate) fn read(r: &mut Reader) -> Result<Self, NifError> {
        let mut translation = [0.0; 3];
        for v in &mut translation {
            *v = r.f32()?;
        }
        let mut rotation = [0.0; 9];
        for v in &mut rotation {
            *v = r.f32()?;
        }
        Ok(Transform {
            translation,
            rotation,
            scale: r.f32()?,
        })
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        for v in self.translation.iter().chain(&self.rotation) {
            w.f32(*v);
        }
        w.f32(self.scale);
    }

    /// A point moved by this transform: scaled, turned, then shifted.
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.rotation;
        let s = self.scale;
        [
            (m[0] * p[0] + m[1] * p[1] + m[2] * p[2]) * s + self.translation[0],
            (m[3] * p[0] + m[4] * p[1] + m[5] * p[2]) * s + self.translation[1],
            (m[6] * p[0] + m[7] * p[1] + m[8] * p[2]) * s + self.translation[2],
        ]
    }
}

/// The start of every scene object.
#[derive(Debug, Clone, PartialEq)]
pub struct AvObject {
    /// Index into the header's strings; -1 for none.
    pub name: i32,
    pub extra_data: Vec<Ref>,
    pub controller: Ref,
    pub flags: u32,
    pub transform: Transform,
    /// Only in files older than Skyrim (stream version 34 and below).
    pub properties: Option<Vec<Ref>>,
    pub collision: Ref,
}

pub(crate) fn read_refs(r: &mut Reader) -> Result<Vec<Ref>, NifError> {
    let n = r.count(4)?;
    (0..n).map(|_| r.i32()).collect()
}

pub(crate) fn write_refs(w: &mut Writer, refs: &[Ref]) {
    w.u32(refs.len() as u32);
    for x in refs {
        w.i32(*x);
    }
}

impl AvObject {
    pub(crate) fn read(r: &mut Reader, stream: u32) -> Result<Self, NifError> {
        let name = r.i32()?;
        let extra_data = read_refs(r)?;
        let controller = r.i32()?;
        let flags = if stream > 26 { r.u32()? } else { r.u16()? as u32 };
        let transform = Transform::read(r)?;
        let properties = if stream <= 34 {
            Some(read_refs(r)?)
        } else {
            None
        };
        Ok(AvObject {
            name,
            extra_data,
            controller,
            flags,
            transform,
            properties,
            collision: r.i32()?,
        })
    }

    pub(crate) fn write(&self, w: &mut Writer, stream: u32) {
        w.i32(self.name);
        write_refs(w, &self.extra_data);
        w.i32(self.controller);
        if stream > 26 {
            w.u32(self.flags);
        } else {
            w.u16(self.flags as u16);
        }
        self.transform.write(w);
        if let Some(p) = &self.properties {
            write_refs(w, p);
        }
        w.i32(self.collision);
    }
}

/// A node: a scene object with children. `NiNode`, and the kinds that add
/// nothing to its start (`BSFadeNode`, `BSLeafAnimNode`).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub av: AvObject,
    pub children: Vec<Ref>,
    /// Not in Fallout 4 and later.
    pub effects: Option<Vec<Ref>>,
    /// Whatever a kind built on a node adds after it, kept as it was.
    pub rest: Vec<u8>,
}

/// The kinds of block that start as a node does.
pub fn is_node(kind: &str) -> bool {
    matches!(
        kind,
        "NiNode"
            | "BSFadeNode"
            | "BSLeafAnimNode"
            | "BSTreeNode"
            | "BSOrderedNode"
            | "BSMultiBoundNode"
            | "BSBlastNode"
            | "BSDamageStage"
            | "BSValueNode"
            | "BSMasterParticleSystem"
            | "NiBillboardNode"
            | "NiSwitchNode"
            | "NiLODNode"
            | "BSDebrisNode"
            | "BSRangeNode"
    )
}

impl Node {
    pub fn read(data: &[u8], stream: u32) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let av = AvObject::read(&mut r, stream)?;
        let children = read_refs(&mut r)?;
        let effects = if stream < 130 {
            Some(read_refs(&mut r)?)
        } else {
            None
        };
        let rest = r.take(r.left())?.to_vec();
        Ok(Node {
            av,
            children,
            effects,
            rest,
        })
    }

    pub fn write(&self, stream: u32) -> Vec<u8> {
        let mut w = Writer::default();
        self.av.write(&mut w, stream);
        write_refs(&mut w, &self.children);
        if let Some(e) = &self.effects {
            write_refs(&mut w, e);
        }
        w.bytes(&self.rest);
        w.out
    }
}
