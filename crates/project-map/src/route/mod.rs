//! Reading a route's points and the dependencies its hops travel along.

use crate::{DependencyId, HopLink, HopTarget, MapRoute, ModuleId, RoutePoint};

impl RoutePoint {
    /// The project module at this point, if it lies in the project.
    pub fn module(&self) -> Option<&ModuleId> {
        match self {
            RoutePoint::Project(point) => Some(&point.module),
            RoutePoint::Outside(_) => None,
        }
    }
}

impl HopTarget {
    /// The project module the hop arrives in, if it lies in the project.
    pub fn module(&self) -> Option<&ModuleId> {
        match self {
            HopTarget::Project { point, .. } => Some(&point.module),
            HopTarget::Outside(_) => None,
        }
    }

    /// The point the hop arrives at, without how it got there.
    pub fn point(&self) -> RoutePoint {
        match self {
            HopTarget::Project { point, .. } => RoutePoint::Project(point.clone()),
            HopTarget::Outside(point) => RoutePoint::Outside(point.clone()),
        }
    }
}

impl MapRoute {
    /// The dependency the hop at `index` travels along. `None` when the hop
    /// arrives outside the project, stays in one module, comes from outside,
    /// or does not exist; also when its link names a sender outside the
    /// project. The result is not checked against `ProjectMap::dependencies`.
    pub fn hop_dependency(&self, index: usize) -> Option<DependencyId> {
        let hop = self.hops.get(index)?;
        let HopTarget::Project { point, link } = &hop.to else {
            return None;
        };
        let receiver = &point.module;
        let sender = match index {
            0 => self.origin.module(),
            _ => self.hops[index - 1].to.module(),
        };
        let (from, to) = match link {
            HopLink::ReceiverUsesSender => (receiver, sender?),
            HopLink::SenderUsesReceiver => (sender?, receiver),
            HopLink::SameModule | HopLink::FromOutside => return None,
        };
        Some(DependencyId {
            from: from.clone(),
            to: to.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
