use super::*;

fn names(names: &[&str]) -> Vec<ClusterName> {
    names
        .iter()
        .map(|name| ClusterName(name.to_string()))
        .collect()
}

#[test]
fn clusters_keep_their_order() {
    let clusters = Clusters::new(names(&["Top", "Middle", "Bottom"])).unwrap();
    let order: Vec<&str> = clusters.iter().map(|cluster| cluster.0.as_str()).collect();
    assert_eq!(order, ["Top", "Middle", "Bottom"]);
}

#[test]
fn a_cluster_listed_twice_is_rejected() {
    assert_eq!(
        Clusters::new(names(&["Top", "Bottom", "Top"])),
        Err(DuplicateCluster(ClusterName("Top".into())))
    );
}
