use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Receiver;
use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use palette::encoding::Linear;
use palette::FromColor;

use crate::hardware::Spis;
use crate::interface::UnwrapInfelliable;

#[derive(Clone, Copy, Default, defmt::Format)]
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

impl<const N: usize> LedStrip<N>
where
    [(); N]:,
    [(); N * 9 * 3]:,
{
    pub fn get_led_data(&self) -> [u8; N * 9 * 3] {
        let mut buffer = [0; N * 9 * 3];

        for index in 0..N {
            let offset = 9 * 3 * index;
            let led = self.leds[index];
            write_value_to_slice(&mut buffer[offset..offset + 9], (led.red * 255.0) as u8);
            write_value_to_slice(&mut buffer[offset + 9..offset + 18], (led.green * 255.0) as u8);
            write_value_to_slice(&mut buffer[offset + 18..offset + 27], (led.blue * 255.0) as u8);
        }

        buffer
    }

    pub fn get_barrier_led_data(&self) -> [u8; N * 9 * 3] {
        let mut buffer = [0; N * 9 * 3];
        let (barrier_leds, amount) = self.barriers.first().unwrap();

        for index in 0..N {
            let offset = 9 * 3 * index;
            let led = self.leds[index];
            let barrier_led = barrier_leds[index];
            write_value_to_slice(
                &mut buffer[offset..offset + 9],
                ((led.red * (1.0 - amount) + barrier_led.red * amount) * 255.0) as u8,
            );
            write_value_to_slice(
                &mut buffer[offset + 9..offset + 18],
                ((led.green * (1.0 - amount) + barrier_led.green * amount) * 255.0) as u8,
            );
            write_value_to_slice(
                &mut buffer[offset + 18..offset + 27],
                ((led.blue * (1.0 - amount) + barrier_led.blue * amount) * 255.0) as u8,
            );
        }

        buffer
    }
}

pub struct Strips {
    top_strip: LedStrip<17>,
}

#[repr(C)]
#[derive(Clone, Copy, defmt::Format)]
pub enum AnimationType {
    Disconnected,
    IndicateMaster { is_master: bool },
    Rainbow,
}

enum Animation {
    None,
    Disconnected { offset: f32 },
    IndicateMaster { is_master: bool },
    Rainbow { hue: f32 },
}

#[embassy_executor::task]
pub async fn led_task(mut spis: Spis<'static>, receiver: Receiver<'static, ThreadModeRawMutex, AnimationType, 3>) -> ! {
    let mut animation = Animation::None;
    let mut strips = Strips {
        top_strip: LedStrip::new(),
    };
    let mut previous_time = embassy_time::Instant::now();

    loop {
        let receive_future = receiver.recv();
        pin_mut!(receive_future);

        loop {
            let timer_future = Timer::after(Duration::from_millis(16));
            pin_mut!(timer_future);

            match select(receive_future, timer_future).await {
                Either::Left((animation_type, _)) => {
                    match animation_type {
                        AnimationType::Disconnected => {
                            animation = Animation::Disconnected { offset: 0.0 };

                            let color = palette::rgb::Rgb::<Linear<f32>, f32>::new(0.0, 0.5, 0.0);
                            let (red, green, blue) = color.into_components();
                            let led = Led::rgb(red, green, blue);

                            strips.top_strip.insert_uniform_barrier(led);
                        }
                        AnimationType::IndicateMaster { is_master } => {
                            animation = Animation::IndicateMaster { is_master };

                            let led = match is_master {
                                true => Led::rgb(1.0, 1.0, 1.0),
                                false => Led::rgb(0.0, 0.0, 0.0),
                            };

                            strips.top_strip.insert_uniform_barrier(led);
                        }
                        AnimationType::Rainbow => {
                            animation = Animation::Rainbow { hue: 0.0 };

                            let color = palette::Hsl::<palette::encoding::Srgb, f32>::new(0.0, 1.0, 0.5);
                            let color = palette::rgb::Rgb::from_color(color);
                            let (red, green, blue) = color.into_linear().into_components();
                            let led = Led::rgb(red, green, blue);

                            strips.top_strip.insert_uniform_barrier(led);
                        }
                    }
                    break;
                }
                Either::Right((_, saved_receive_future)) => {
                    let current_time = embassy_time::Instant::now();
                    let elapsed_time = (current_time - previous_time).as_millis() as f32 / 1000.0;
                    previous_time = current_time;

                    if strips.top_strip.update_barrier(elapsed_time) {
                        if let Some(spi) = &mut spis.spi_2 {
                            defmt::unwrap!(spi.write(&strips.top_strip.get_barrier_led_data()).await);
                        }
                    } else {
                        match &mut animation {
                            Animation::None => {
                                strips.top_strip.set_uniform_color(Led::rgb(0.0, 0.0, 0.0));
                            }
                            Animation::Disconnected { offset } => {
                                *offset += 3.0 * elapsed_time;
                                // Between 0 and 1
                                let brightness = 0.5 + (libm::sin(*offset as f64) * 0.5);
                                let color = palette::rgb::Rgb::<Linear<f32>, f32>::new(0.0, brightness as f32, 0.0);
                                let (red, green, blue) = color.into_components();
                                let led = Led::rgb(red, green, blue);

                                strips.top_strip.set_uniform_color(led);
                            }
                            Animation::IndicateMaster { is_master } => {
                                let led = match is_master {
                                    true => Led::rgb(1.0, 1.0, 1.0),
                                    false => Led::rgb(0.0, 0.0, 0.0),
                                };

                                strips.top_strip.set_uniform_color(led);
                            }
                            Animation::Rainbow { hue } => {
                                *hue += 30.0 * elapsed_time;

                                let color = palette::Hsl::<palette::encoding::Srgb, f32>::new(*hue, 1.0, 0.5);
                                let color = palette::rgb::Rgb::from_color(color);
                                let (red, green, blue) = color.into_linear().into_components();
                                let led = Led::rgb(red, green, blue);

                                strips.top_strip.set_uniform_color(led);
                            }
                        }

                        if let Some(spi) = &mut spis.spi_2 {
                            defmt::unwrap!(spi.write(&strips.top_strip.get_led_data()).await);
                        }
                    }

                    receive_future = saved_receive_future;
                }
            }
        }
    }
}
