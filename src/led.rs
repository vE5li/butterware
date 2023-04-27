#[derive(Clone, Copy, Default)]
pub struct Led {
    red: u8,
    green: u8,
    blue: u8,
}

impl Led {
    pub const fn off() -> Self {
        Self { red: 0, green: 0, blue: 0 }
    }

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

pub struct LedStrip<const N: usize>
where
    [(); N]:,
{
    leds: [Led; N],
}

impl<const N: usize> LedStrip<N>
where
    [(); N]:,
{
    const OFF_LED: Led = Led::off();

    pub const fn new() -> Self {
        Self { leds: [Self::OFF_LED; N] }
    }

    pub fn set_uniform_color(&mut self, led: Led) {
        self.leds = [led; N];
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
            write_value_to_slice(&mut buffer[offset..offset + 9], led.red);
            write_value_to_slice(&mut buffer[offset + 9..offset + 18], led.green);
            write_value_to_slice(&mut buffer[offset + 18..offset + 27], led.blue);
        }

        buffer
    }
}

/*spi.write(&[
    //green (0)
    0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011, 0b11110001, 0b11111000,
    // red (255)
    0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011, 0b11110001, 0b11111000,
    // blue (255)
    0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
])*/
