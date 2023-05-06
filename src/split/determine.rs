use nrf_softdevice::ble::{central, gatt_server, peripheral, Address};
use nrf_softdevice::Softdevice;

use crate::ble::{MasterServer, MasterServerEvent, MasterServiceClient, MasterServiceEvent};
use crate::hardware::generate_random_u32;

#[allow(dead_code)]
pub async fn advertise_determine_master(softdevice: &Softdevice, server: &MasterServer, adv_data: &[u8], scan_data: &[u8]) -> bool {
    let config = peripheral::Config::default();
    let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };

    defmt::debug!("start advertising");

    let connection = defmt::unwrap!(peripheral::advertise_connectable(softdevice, adv, &config).await);

    defmt::debug!("connected to other half");

    let random_number = generate_random_u32(softdevice).await;

    defmt::debug!("random number is {}", random_number);

    let mut is_master = false;
    let _ = gatt_server::run(&connection, server, |event| match event {
        MasterServerEvent::MasterService(event) => match event {
            MasterServiceEvent::OtherRandomNumberWrite(other_random_number) => {
                // Determine which side is the master based on our random numbers.
                is_master = random_number > other_random_number;

                defmt::debug!("other random number is {}", other_random_number);

                // Update is_master so that the other side can read it.
                defmt::unwrap!(server.master_service.is_master_set(&is_master));
            }
        },
    })
    .await;

    is_master
}

#[allow(dead_code)]
pub async fn connect_determine_master(softdevice: &Softdevice, address: &Address) -> bool {
    let addresses = [address];
    let mut config = central::ConnectConfig::default();
    config.scan_config.whitelist = Some(&addresses);
    config.conn_params.min_conn_interval = 6;
    config.conn_params.max_conn_interval = 6;

    defmt::debug!("start scanning");

    let connection = defmt::unwrap!(central::connect(softdevice, &config).await);
    let client: MasterServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&connection).await);

    defmt::debug!("connected to other half");

    let random_number = generate_random_u32(softdevice).await;

    defmt::debug!("random number is {}", random_number);
    defmt::debug!("writing random number to the master service");

    defmt::unwrap!(client.other_random_number_write(&random_number).await);

    defmt::debug!("reading is_master from the master service");

    !defmt::unwrap!(client.is_master_read().await)
}
