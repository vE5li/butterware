use embassy_nrf::saadc::{ChannelConfig, Config, Oversample, Saadc, VddhDiv5Input};
use embassy_nrf::{bind_interrupts, saadc, Peripherals};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Receiver};
use embassy_time::Timer;

use crate::interface::Keyboard;

bind_interrupts!(struct Irqs {
    SAADC => saadc::InterruptHandler;
});

pub struct BatteryLevel(pub u8);
pub struct Voltage(pub f32);

const BATTERY_CHANNEL_SIZE: usize = 2;

static BATTERY_CHANNEL: Channel<ThreadModeRawMutex, BatteryLevel, BATTERY_CHANNEL_SIZE> = Channel::new();

pub type BatteryLevelReceiver = Receiver<'static, ThreadModeRawMutex, BatteryLevel, BATTERY_CHANNEL_SIZE>;

pub fn battery_level_receiver() -> BatteryLevelReceiver {
    BATTERY_CHANNEL.receiver()
}

#[embassy_executor::task]
pub async fn battery_task() {
    // TODO: not steal but get from keyboard setup (?)
    let peripherals = unsafe { Peripherals::steal() };
    let channel_config = ChannelConfig::single_ended(VddhDiv5Input);
    let mut config = Config::default();
    config.oversample = Oversample::OVER32X;

    let mut saadc = Saadc::new(peripherals.SAADC, Irqs, config, [channel_config]);

    loop {
        let mut buffer = [0; 1];

        saadc.calibrate().await;
        saadc.sample(&mut buffer).await;

        let battery_percentage = calculate_battery_percentage(buffer[0]) as u8;

        defmt::info!("current battery percentage: {}", battery_percentage);

        BATTERY_CHANNEL.send(BatteryLevel(battery_percentage)).await;

        Timer::after(<crate::Used as Keyboard>::BATTERY_SAMPLE_FREQUENCY).await;
    }
}

fn calculate_battery_percentage(raw_value: i16) -> f32 {
    // TODO: move to const
    let adc_resolution_bits = 12;
    // TODO: move to const
    let adc_reference_voltage = 0.6;

    let adc_gain = (1.0 / 6.0) as f32;
    let adc_range = (1 << adc_resolution_bits) as f32;

    let adc_resolution = adc_reference_voltage / (adc_range * adc_gain);

    // Since our reference it VDD / 5 we need to multiply by 5 here to get the
    // actual voltage.
    let current_voltage = (raw_value.abs() as f32) * 5.0 * adc_resolution;

    // Voltage limits for the battery.
    let minimum_voltage = <crate::Used as Keyboard>::BATTERY_MINIMUM_VOLTAGE.0;
    let maximum_voltage = <crate::Used as Keyboard>::BATTERY_MAXIMUM_VOLTAGE.0;

    ((current_voltage - minimum_voltage) / (maximum_voltage - minimum_voltage)) * 100.0
}
