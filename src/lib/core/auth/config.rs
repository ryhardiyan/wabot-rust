use whatsapp_rust::pair_code::PairCodeOptions;

pub fn pairing(config: &crate::config::Config) -> Option<PairCodeOptions> {
    (!config.pairing_number.is_empty()).then(|| PairCodeOptions {
        phone_number: config.pairing_number.clone(),
        custom_code: config.pairing_code.clone(),
        ..Default::default()
    })
}
