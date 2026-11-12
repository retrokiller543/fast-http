#[test]
fn modules_are_reachable_through_the_facade() {
    let _ = std::any::type_name::<crate::router::Router<()>>();
    let _ = std::any::type_name::<crate::server::Server<crate::transport::Memory>>();
    let _ = std::any::type_name::<crate::layer::Identity>();
}

#[cfg(feature = "ws")]
#[test]
fn ws_is_reachable_with_its_feature() {
    let _ = std::any::type_name::<crate::ws::WebSocketUpgrade>();
}

#[cfg(feature = "multipart")]
#[test]
fn multipart_is_reachable_with_its_feature() {
    let _ = std::any::type_name::<crate::multipart::Multipart>();
}
