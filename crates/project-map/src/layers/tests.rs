use super::*;

fn names(names: &[&str]) -> Vec<LayerName> {
    names
        .iter()
        .map(|name| LayerName(name.to_string()))
        .collect()
}

#[test]
fn layers_keep_their_order() {
    let layers = Layers::new(names(&["Top", "Middle", "Bottom"])).unwrap();
    let order: Vec<&str> = layers.iter().map(|layer| layer.0.as_str()).collect();
    assert_eq!(order, ["Top", "Middle", "Bottom"]);
}

#[test]
fn a_layer_listed_twice_is_rejected() {
    assert_eq!(
        Layers::new(names(&["Top", "Bottom", "Top"])),
        Err(DuplicateLayer(LayerName("Top".into())))
    );
}
