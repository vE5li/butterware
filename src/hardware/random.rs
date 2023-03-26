use embassy_time::{Duration, Timer};
use nrf_softdevice::{random_bytes, Softdevice};

pub async fn generate_random_u32(softdevice: &Softdevice) -> u32 {
    loop {
        let mut buffer = [0; 4];

        if let Ok(()) = random_bytes(softdevice, &mut buffer) {
            return u32::from_le_bytes(buffer);
        }

        Timer::after(Duration::from_millis(5)).await;
    }
}
