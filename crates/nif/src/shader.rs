//! What a shape is drawn with: enough of the shader property to find its
//! textures, and the texture set that names them.

use crate::io::{Reader, Writer};
use crate::node::{read_refs, write_refs, Ref};
use crate::NifError;

/// The start of a `BSLightingShaderProperty`: everything up to the link to
/// its textures. The rest is kept as it was.
#[derive(Debug, Clone, PartialEq)]
pub struct LightingShader {
    /// Skyrim and Special Edition say which kind of shader first.
    pub shader_type: Option<u32>,
    pub name: i32,
    pub extra_data: Vec<Ref>,
    pub controller: Ref,
    pub flags1: u32,
    pub flags2: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub texture_set: Ref,
    pub rest: Vec<u8>,
}

impl LightingShader {
    pub fn read(data: &[u8], stream: u32) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let shader_type = if (83..=100).contains(&stream) {
            Some(r.u32()?)
        } else {
            None
        };
        let name = r.i32()?;
        let extra_data = read_refs(&mut r)?;
        let controller = r.i32()?;
        let flags1 = r.u32()?;
        let flags2 = r.u32()?;
        let uv_offset = [r.f32()?, r.f32()?];
        let uv_scale = [r.f32()?, r.f32()?];
        let texture_set = r.i32()?;
        let rest = r.take(r.left())?.to_vec();
        Ok(LightingShader {
            shader_type,
            name,
            extra_data,
            controller,
            flags1,
            flags2,
            uv_offset,
            uv_scale,
            texture_set,
            rest,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        if let Some(t) = self.shader_type {
            w.u32(t);
        }
        w.i32(self.name);
        write_refs(&mut w, &self.extra_data);
        w.i32(self.controller);
        w.u32(self.flags1);
        w.u32(self.flags2);
        for v in self.uv_offset.iter().chain(&self.uv_scale) {
            w.f32(*v);
        }
        w.i32(self.texture_set);
        w.bytes(&self.rest);
        w.out
    }
}

/// `BSShaderTextureSet`: the texture files, by slot. Slot 0 is the colour,
/// slot 1 the normal map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureSet {
    pub textures: Vec<Vec<u8>>,
}

impl TextureSet {
    pub fn read(data: &[u8]) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let n = r.count(4)?;
        let mut textures = Vec::with_capacity(n);
        for _ in 0..n {
            textures.push(r.sized_string()?);
        }
        if r.left() != 0 {
            return Err(NifError::Corrupt("bytes after a texture set".into()));
        }
        Ok(TextureSet { textures })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.textures.len() as u32);
        for t in &self.textures {
            w.sized_string(t);
        }
        w.out
    }
}
