//! An ordered list of layer names with no repeats.

use crate::LayerName;

/// The map's layers from top to bottom, each listed once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layers(Vec<LayerName>);

impl Layers {
    /// A map without layers.
    pub fn empty() -> Self {
        Layers(vec![])
    }

    /// The layers in the given order. Fails on the first layer listed twice.
    pub fn new(layers: Vec<LayerName>) -> Result<Self, DuplicateLayer> {
        for (index, layer) in layers.iter().enumerate() {
            if layers[..index].contains(layer) {
                return Err(DuplicateLayer(layer.clone()));
            }
        }
        Ok(Layers(layers))
    }

    /// The layers from top to bottom.
    pub fn iter(&self) -> std::slice::Iter<'_, LayerName> {
        self.0.iter()
    }

    pub fn contains(&self, layer: &LayerName) -> bool {
        self.0.contains(layer)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A layer that was listed more than once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateLayer(pub LayerName);

impl std::fmt::Display for DuplicateLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "layer {:?} is listed more than once", self.0.0)
    }
}

impl std::error::Error for DuplicateLayer {}

#[cfg(test)]
mod tests;
