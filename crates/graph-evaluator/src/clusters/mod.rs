//! An ordered list of cluster names with no repeats.

use crate::ClusterName;

/// The evaluation's clusters in reading order, each listed once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clusters(Vec<ClusterName>);

impl Clusters {
    /// An evaluation without clusters.
    pub fn empty() -> Self {
        Clusters(vec![])
    }

    /// The clusters in the given order. Fails on the first cluster listed twice.
    pub fn new(clusters: Vec<ClusterName>) -> Result<Self, DuplicateCluster> {
        for (index, cluster) in clusters.iter().enumerate() {
            if clusters[..index].contains(cluster) {
                return Err(DuplicateCluster(cluster.clone()));
            }
        }
        Ok(Clusters(clusters))
    }

    /// The clusters in reading order.
    pub fn iter(&self) -> std::slice::Iter<'_, ClusterName> {
        self.0.iter()
    }

    pub fn contains(&self, cluster: &ClusterName) -> bool {
        self.0.contains(cluster)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A cluster that was listed more than once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateCluster(pub ClusterName);

impl std::fmt::Display for DuplicateCluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cluster {:?} is listed more than once", self.0.0)
    }
}

impl std::error::Error for DuplicateCluster {}

#[cfg(test)]
mod tests;
