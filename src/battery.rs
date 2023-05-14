use embassy_nrf::saadc::{ChannelConfig, Config, Oversample, Saadc, VddhDiv5Input};
use embassy_nrf::{bind_interrupts, saadc, Peripherals};
use embassy_time::{Duration, Timer};

use crate::ble::Server;

bind_interrupts!(struct Irqs {
    SAADC => saadc::InterruptHandler;
});

#[embassy_executor::task]
pub async fn battery_task(server: &'static Server) {
    let p = unsafe { Peripherals::steal() };
    let channel_config = ChannelConfig::single_ended(VddhDiv5Input);
    let mut config = Config::default();
    config.oversample = Oversample::OVER32X;

    let mut saadc = Saadc::new(p.SAADC, Irqs, config, [channel_config]);

    loop {
        saadc.calibrate().await;

        let mut buf = [0; 1];
        saadc.sample(&mut buf).await;
        let battery_percentage = calculate_battery_percentage(buf[0]) as u8;

        defmt::info!("sample: {=i16}", &buf[0]);
        defmt::info!("percentage: {}", battery_percentage);

        // Notify instead
        server.battery_service.battery_level_set(&battery_percentage);

        // TODO: less often
        Timer::after(Duration::from_secs(20)).await;
    }
}

fn calculate_battery_percentage(raw_value: i16) -> f32 {
    // Define your ADC resolution in bits
    let adc_resolution_bits = 12;

    // Define your ADC reference voltage (in volts)
    let adc_reference_voltage = 0.6;

    // Define your battery voltage range (Vmin and Vmax)
    let battery_voltage_min = 3.2;
    let battery_voltage_max = 4.2;

    // Define your ADC gain
    let adc_gain = 6;

    // Calculate the ADC resolution based on the provided bits
    let adc_resolution = adc_reference_voltage / ((libm::powf(2.0f32, adc_resolution_bits as f32) - 1.0) * (1.0 / adc_gain as f32));

    defmt::info!("adc resolution: {}", adc_resolution);

    // Calculate the battery voltage based on the raw SAADC value
    let voltage = (raw_value.abs() as f32) * 5.0 * adc_resolution;

    defmt::info!("voltage: {}", voltage);

    // Calculate the battery percentage
    let battery_percentage = ((voltage - battery_voltage_min) / (battery_voltage_max - battery_voltage_min)) * 100.0;

    defmt::info!("battery_percentage: {}", voltage);

    battery_percentage
}
