use embassy_nrf::gpio::AnyPin;
use embassy_nrf::spim::Spim;
use embassy_nrf::{peripherals, spim};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::FixedGattValue;
use palette::FromColor;

use crate::interface::{Keyboard, UnwrapInfelliable};
use crate::Side;

#[repr(C)]
#[derive(Clone, defmt::Format)]
pub enum LightingOperation {
    SetAnimation { index: LedIndex, animation: Animation },
}

impl FixedGattValue for LightingOperation {
    const SIZE: usize = core::mem::size_of::<LightingOperation>();

    fn from_gatt(data: &[u8]) -> Self {
        let mut buffer = [0; Self::SIZE];
        buffer.copy_from_slice(data);
        unsafe { core::mem::transmute::<&[u8; Self::SIZE], &LightingOperation>(&buffer).clone() }
    }

    fn to_gatt(&self) -> &[u8] {
        unsafe { core::mem::transmute::<&LightingOperation, &[u8; Self::SIZE]>(self) }
    }
}

pub async fn set_animation(side: Side, index: LedIndex, animation: Animation) {
    let lighting_operation = LightingOperation::SetAnimation { index, animation };

    if side.includes_this() {
        LIGHTING_OPERATIONS.send(lighting_operation.clone()).await;
    }

    if side.includes_other() {
        OTHER_LIGHTING_OPERATIONS.send(lighting_operation).await;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format, PartialEq)]
pub struct Led {
    red: f32,
    green: f32,
    blue: f32,
}

impl Led {
    pub const fn off() -> Self {
        Self {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        }
    }

    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }
}

pub struct LedStrip<const N: usize>
where
    [(); N]:,
{
    leds: [Led; N],
    barriers: heapless::Vec<([Led; N], f32), 2>,
}

impl<const N: usize> LedStrip<N>
where
    [(); N]:,
{
    const OFF_LED: Led = Led::off();

    pub const fn new() -> Self {
        Self {
            leds: [Self::OFF_LED; N],
            barriers: heapless::Vec::new(),
        }
    }

    pub fn set_uniform_color(&mut self, led: Led) {
        self.leds = [led; N];
    }

    pub fn insert_uniform_barrier(&mut self, led: Led) {
        match self.barriers.len() < 2 {
            true => self.barriers.push(([led; N], 0.0)).unwrap_infelliable(),
            false => *self.barriers.last_mut().unwrap_infelliable() = ([led; N], 0.0),
        }
    }

    pub fn update_barrier(&mut self, elapsed_time: f32) -> bool {
        let complete = if let Some((_leds, timer)) = self.barriers.first_mut() {
            *timer += 3.0 * elapsed_time;
            *timer >= 1.0
        } else {
            false
        };

        if complete {
            let (leds, _) = self.barriers.remove(0);
            self.leds = leds;
        }

        !self.barriers.is_empty()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format, PartialEq)]
pub struct Speed(pub f32);

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format, PartialEq)]
pub enum Animation {
    // This first animation is the set after flashing.
    Static { color: Led },
    Pulsate { color: Led, speed: Speed, offset: f32 },
    Rainbow { hue: f32, speed: Speed },
}

const LIGHTING_CHANNEL_SIZE: usize = 10;

pub type UsedLeds = <<crate::Used as Keyboard>::Leds as LedProvider>::Collection;
pub type LedIndex = <UsedLeds as LedCollection>::Index;
pub type LightingSender = Sender<'static, ThreadModeRawMutex, LightingOperation, LIGHTING_CHANNEL_SIZE>;
pub type OtherLightingReceiver = Receiver<'static, ThreadModeRawMutex, LightingOperation, LIGHTING_CHANNEL_SIZE>;

static LIGHTING_OPERATIONS: Channel<ThreadModeRawMutex, LightingOperation, LIGHTING_CHANNEL_SIZE> = Channel::new();
static OTHER_LIGHTING_OPERATIONS: Channel<ThreadModeRawMutex, LightingOperation, LIGHTING_CHANNEL_SIZE> = Channel::new();

pub fn lighting_sender() -> LightingSender {
    LIGHTING_OPERATIONS.sender()
}

pub fn other_lighting_receiver() -> OtherLightingReceiver {
    OTHER_LIGHTING_OPERATIONS.receiver()
}

#[embassy_executor::task]
pub async fn lighting_task(mut leds: UsedLeds) -> ! {
    let mut previous_time = embassy_time::Instant::now();
    let receiver = LIGHTING_OPERATIONS.receiver();

    loop {
        let receive_future = receiver.recv();
        pin_mut!(receive_future);

        loop {
            let timer_future = Timer::after(Duration::from_millis(16));
            pin_mut!(timer_future);

            match select(receive_future, timer_future).await {
                Either::Left((lighting_operation, _)) => {
                    match lighting_operation {
                        LightingOperation::SetAnimation { index, animation } => leds.set_animation(index, animation),
                    }
                    break;
                }
                Either::Right((_, saved_receive_future)) => {
                    let current_time = embassy_time::Instant::now();
                    let elapsed_time = (current_time - previous_time).as_millis() as f32 / 1000.0;
                    previous_time = current_time;

                    leds.update(elapsed_time).await;
                    receive_future = saved_receive_future;
                }
            }
        }
    }
}

pub trait LedProvider {
    type Collection: LedCollection;
}

pub trait LedCollection {
    type Index: Clone;

    fn set_animation(&mut self, index: Self::Index, animation: Animation);

    async fn update(&mut self, elapsed_time: f32);
}

pub trait LedDriver {
    fn set_animation(&mut self, animation: Animation);

    async fn update(&mut self, elapsed_time: f32);
}

embassy_nrf::bind_interrupts!(pub struct Irqs {
    SPIM3 => spim::InterruptHandler<peripherals::SPI3>;
    SPIM2_SPIS2_SPI2 => spim::InterruptHandler<peripherals::SPI2>;
    SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1 => spim::InterruptHandler<peripherals::TWISPI1>;
});

#[allow(unused)]
pub struct Ws2812bDriver<const C: usize, SPI: spim::Instance> {
    strip: LedStrip<C>,
    spi: Spim<'static, SPI>,
    phantom_data: core::marker::PhantomData<SPI>,
    current_animation: Animation,
}

impl<const C: usize, SPI: spim::Instance> Ws2812bDriver<C, SPI>
where
    Irqs: embassy_cortex_m::interrupt::Binding<<SPI as embassy_nrf::spim::Instance>::Interrupt, embassy_nrf::spim::InterruptHandler<SPI>>,
{
    #[allow(unused)]
    pub fn new(mosi_pin: AnyPin, clock_pin: AnyPin, spi: SPI) -> Self {
        let mut config = embassy_nrf::spim::Config::default();
        config.frequency = embassy_nrf::spim::Frequency::M8;
        config.mode = embassy_nrf::spim::MODE_1;

        let initial_animation = Animation::Static {
            color: Led::rgb(0.0, 0.0, 0.0),
        };

        Self {
            strip: LedStrip::new(),
            spi: Spim::new_txonly(spi, Irqs, clock_pin, mosi_pin, config),
            current_animation: initial_animation,
            phantom_data: core::marker::PhantomData,
        }
    }
}

impl<const C: usize, SPI: spim::Instance> Ws2812bDriver<C, SPI>
where
    [(); C]:,
    [(); C * 9 * 3]:,
{
    fn write_value_to_slice(slice: &mut [u8], value: u8) {
        const LOOKUP: [u8; 2] = [0b000, 0b111];

        let test_bit = |offset: usize| ((value >> offset) & 0b1) as usize;

        slice[8] = 0b11000000 | (LOOKUP[test_bit(0)] << 3);
        slice[7] = 0b10000001 | (LOOKUP[test_bit(1)] << 4);
        slice[6] = 0b00000011 | (LOOKUP[test_bit(2)] << 5);
        slice[5] = 0b00000111 | (LOOKUP[test_bit(3)] << 6);
        slice[4] = 0b00001110 | (LOOKUP[test_bit(4)] << 7) | (LOOKUP[test_bit(3)] >> 2);
        slice[3] = 0b00011100 | (LOOKUP[test_bit(4)] >> 1);
        slice[2] = 0b00111000 | (LOOKUP[test_bit(5)]);
        slice[1] = 0b01110000 | (LOOKUP[test_bit(6)] << 1);
        slice[0] = 0b11100000 | (LOOKUP[test_bit(7)] << 2);
    }

    pub fn get_led_data(&self) -> [u8; C * 9 * 3] {
        let mut buffer = [0; C * 9 * 3];

        for index in 0..C {
            let offset = 9 * 3 * index;
            let led = self.strip.leds[index];

            Self::write_value_to_slice(&mut buffer[offset..offset + 9], (led.green * 255.0) as u8);
            Self::write_value_to_slice(&mut buffer[offset + 9..offset + 18], (led.red * 255.0) as u8);
            Self::write_value_to_slice(&mut buffer[offset + 18..offset + 27], (led.blue * 255.0) as u8);
        }

        buffer
    }

    pub fn get_barrier_led_data(&self) -> [u8; C * 9 * 3] {
        let mut buffer = [0; C * 9 * 3];
        let (barrier_leds, amount) = self.strip.barriers.first().unwrap();

        for index in 0..C {
            let offset = 9 * 3 * index;
            let led = self.strip.leds[index];
            let barrier_led = barrier_leds[index];
            Self::write_value_to_slice(
                &mut buffer[offset..offset + 9],
                ((led.green * (1.0 - amount) + barrier_led.green * amount) * 255.0) as u8,
            );
            Self::write_value_to_slice(
                &mut buffer[offset + 9..offset + 18],
                ((led.red * (1.0 - amount) + barrier_led.red * amount) * 255.0) as u8,
            );
            Self::write_value_to_slice(
                &mut buffer[offset + 18..offset + 27],
                ((led.blue * (1.0 - amount) + barrier_led.blue * amount) * 255.0) as u8,
            );
        }

        buffer
    }
}

impl<const C: usize, SPI: spim::Instance> LedDriver for Ws2812bDriver<C, SPI>
where
    [(); C]:,
    [(); C * 9 * 3]:,
{
    fn set_animation(&mut self, animation: Animation) {
        match animation {
            Animation::Static { color } => {
                self.strip.insert_uniform_barrier(color);
            }
            Animation::Pulsate { color, offset, .. } => {
                // Between 0 and 1
                let brightness = 0.5 + (libm::sin(offset as f64) * 0.5);
                let led = Led::rgb(
                    color.red * brightness as f32,
                    color.green * brightness as f32,
                    color.blue * brightness as f32,
                );

                self.strip.insert_uniform_barrier(led);
            }
            Animation::Rainbow { hue, .. } => {
                let color = palette::Hsl::<palette::encoding::Srgb, f32>::new(hue, 1.0, 0.5);
                let color = palette::rgb::Rgb::from_color(color);
                let (red, green, blue) = color.into_linear().into_components();
                let led = Led::rgb(red, green, blue);

                self.strip.insert_uniform_barrier(led);
            }
        }

        self.current_animation = animation;
    }

    async fn update(&mut self, elapsed_time: f32) {
        if self.strip.update_barrier(elapsed_time) {
            let _ = self.spi.write(&self.get_barrier_led_data()).await;
        } else {
            match &mut self.current_animation {
                Animation::Static { .. } => {}
                Animation::Pulsate { offset, color, speed } => {
                    *offset += speed.0 * elapsed_time;
                    // Between 0 and 1
                    let brightness = 0.5 + (libm::sin(*offset as f64) * 0.5);
                    let led = Led::rgb(
                        color.red * brightness as f32,
                        color.green * brightness as f32,
                        color.blue * brightness as f32,
                    );

                    self.strip.set_uniform_color(led);
                }
                Animation::Rainbow { hue, speed } => {
                    *hue += speed.0 * elapsed_time;

                    let color = palette::Hsl::<palette::encoding::Srgb, f32>::new(*hue, 1.0, 0.5);
                    let color = palette::rgb::Rgb::from_color(color);
                    let (red, green, blue) = color.into_linear().into_components();
                    let led = Led::rgb(red, green, blue);

                    self.strip.set_uniform_color(led);
                }
            }

            let _ = self.spi.write(&self.get_led_data()).await;
        }
    }
}

#[allow(unused)]
pub struct Sk6812Driver<const C: usize, SPI: spim::Instance> {
    strip: LedStrip<C>,
    spi: Spim<'static, SPI>,
    phantom_data: core::marker::PhantomData<SPI>,
    current_animation: Animation,
}

impl<const C: usize, SPI: spim::Instance> Sk6812Driver<C, SPI>
where
    Irqs: embassy_cortex_m::interrupt::Binding<<SPI as embassy_nrf::spim::Instance>::Interrupt, embassy_nrf::spim::InterruptHandler<SPI>>,
{
    #[allow(unused)]
    pub fn new(mosi_pin: AnyPin, clock_pin: AnyPin, spi: SPI) -> Self {
        let mut config = embassy_nrf::spim::Config::default();
        config.frequency = embassy_nrf::spim::Frequency::M4;
        config.mode = embassy_nrf::spim::MODE_1;

        let initial_animation = Animation::Static {
            color: Led::rgb(0.0, 0.0, 0.0),
        };

        Self {
            strip: LedStrip::new(),
            spi: Spim::new_txonly(spi, Irqs, clock_pin, mosi_pin, config),
            current_animation: initial_animation,
            phantom_data: core::marker::PhantomData,
        }
    }
}

impl<const C: usize, SPI: spim::Instance> Sk6812Driver<C, SPI>
where
    [(); C]:,
    [(); C * 5 * 3]:,
{
    fn write_value_to_slice(slice: &mut [u8], value: u8) {
        const LOOKUP: [u8; 2] = [0b0, 0b1];

        let test_bit = |offset: usize| ((value >> offset) & 0b1) as usize;

        // 4 -> 00011000
        // 3 -> 01100011
        // 2 -> 10001100
        // 1 -> 00110001
        // 0 -> 11000110

        slice[4] = 0b10000100 | (LOOKUP[test_bit(0)] << 6) | (LOOKUP[test_bit(1)] << 1);
        slice[3] = 0b00100001 | (LOOKUP[test_bit(2)] << 4);
        slice[2] = 0b00001000 | (LOOKUP[test_bit(3)] << 7) | (LOOKUP[test_bit(4)] << 2);
        slice[1] = 0b01000010 | (LOOKUP[test_bit(5)] << 5) | LOOKUP[test_bit(6)];
        slice[0] = 0b00010000 | (LOOKUP[test_bit(7)] << 3);
    }

    pub fn get_led_data(&self) -> [u8; C * 5 * 3] {
        let mut buffer = [0; C * 5 * 3];

        for index in 0..C {
            let offset = 5 * 3 * index;
            let led = self.strip.leds[index];
            Self::write_value_to_slice(&mut buffer[offset..offset + 5], (led.red * 255.0) as u8);
            Self::write_value_to_slice(&mut buffer[offset + 5..offset + 10], (led.green * 255.0) as u8);
            Self::write_value_to_slice(&mut buffer[offset + 10..offset + 15], (led.blue * 255.0) as u8);
        }

        buffer
    }

    pub fn get_barrier_led_data(&self) -> [u8; C * 5 * 3] {
        let mut buffer = [0; C * 5 * 3];
        let (barrier_leds, amount) = self.strip.barriers.first().unwrap();

        for index in 0..C {
            let offset = 5 * 3 * index;
            let led = self.strip.leds[index];
            let barrier_led = barrier_leds[index];
            Self::write_value_to_slice(
                &mut buffer[offset..offset + 5],
                ((led.red * (1.0 - amount) + barrier_led.red * amount) * 255.0) as u8,
            );
            Self::write_value_to_slice(
                &mut buffer[offset + 5..offset + 10],
                ((led.green * (1.0 - amount) + barrier_led.green * amount) * 255.0) as u8,
            );
            Self::write_value_to_slice(
                &mut buffer[offset + 10..offset + 15],
                ((led.blue * (1.0 - amount) + barrier_led.blue * amount) * 255.0) as u8,
            );
        }

        buffer
    }
}

impl<const C: usize, SPI: spim::Instance> LedDriver for Sk6812Driver<C, SPI>
where
    [(); C]:,
    [(); C * 5 * 3]:,
{
    fn set_animation(&mut self, animation: Animation) {
        match animation {
            Animation::Static { color } => {
                self.strip.insert_uniform_barrier(color);
            }
            Animation::Pulsate { color, offset, .. } => {
                // Between 0 and 1
                let brightness = 0.5 + (libm::sin(offset as f64) * 0.5);
                let led = Led::rgb(
                    color.red * brightness as f32,
                    color.green * brightness as f32,
                    color.blue * brightness as f32,
                );

                self.strip.insert_uniform_barrier(led);
            }
            Animation::Rainbow { hue, .. } => {
                let color = palette::Hsl::<palette::encoding::Srgb, f32>::new(hue, 1.0, 0.5);
                let color = palette::rgb::Rgb::from_color(color);
                let (red, green, blue) = color.into_linear().into_components();
                let led = Led::rgb(red, green, blue);

                self.strip.insert_uniform_barrier(led);
            }
        }

        self.current_animation = animation;
    }

    async fn update(&mut self, elapsed_time: f32) {
        if self.strip.update_barrier(elapsed_time) {
            let _ = self.spi.write(&self.get_barrier_led_data()).await;
        } else {
            match &mut self.current_animation {
                Animation::Static { .. } => {}
                Animation::Pulsate { offset, color, speed } => {
                    *offset += speed.0 * elapsed_time;
                    // Between 0 and 1
                    let brightness = 0.5 + (libm::sin(*offset as f64) * 0.5);
                    let led = Led::rgb(
                        color.red * brightness as f32,
                        color.green * brightness as f32,
                        color.blue * brightness as f32,
                    );

                    self.strip.set_uniform_color(led);
                }
                Animation::Rainbow { hue, speed } => {
                    *hue += speed.0 * elapsed_time;

                    let color = palette::Hsl::<palette::encoding::Srgb, f32>::new(*hue, 1.0, 0.5);
                    let color = palette::rgb::Rgb::from_color(color);
                    let (red, green, blue) = color.into_linear().into_components();
                    let led = Led::rgb(red, green, blue);

                    self.strip.set_uniform_color(led);
                }
            }

            let _ = self.spi.write(&self.get_led_data()).await;
        }
    }
}
