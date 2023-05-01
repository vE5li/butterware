use embassy_time::{Duration, Timer};
use nrf_softdevice::{random_bytes, Softdevice};

pub async fn generate_random_u32(softdevice: &Softdevice) -> u32 {
    loop {
        let mut count = 0u8;
        unsafe { nrf_softdevice::raw::sd_rand_application_bytes_available_get(&mut count as *mut u8) };

        if count >= 4 {
            let mut buffer = [0; 4];
            let result = random_bytes(softdevice, &mut buffer);
            return u32::from_le_bytes(buffer);
        }

        Timer::after(Duration::from_millis(5)).await;
    }
}
