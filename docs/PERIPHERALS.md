# Peripherals and runtime behavior

[Project overview](../README.md) | [Getting started](GETTING-STARTED.md)

The [verification overview](GETTING-STARTED.md#understand-what-the-checks-cover) explains the named checks below and the deliberately modified test controls.

## Find the board's pins and buses

A program can query its board with `new Lamella.Hardware.TargetBoard()`.
It returns the board's name, chip, clock, and pins using the labels printed on the board.
Each pin that can carry PWM includes its two PWM numbers, the `PwmChip` and `PwmChannelNumber` properties of `BoardPin`: the two values `PwmChannel.Create` takes.
On this system the chip is 0 and the channel number is the pin's own number.
It also returns the buses and the rates the image supports.
`CreatePwmChannel(0, pin)` opens the standard PWM channel on a pin with that function, in the standard board class's form (the chip, then the channel), and `CreateAnalogController(0)` returns the standard analog controller over the chip's converter.
See [Build a .NET program](GETTING-STARTED.md#build-a-net-program) for where the package `Lamella.Hardware` and the SDK come from.

## Use the standard GPIO package

A program's `new GpioController()` from `System.Device.Gpio` runs unchanged.
The package's Linux driver binds the libgpiod v1 API.
The hosting layer maps that API to kernel operations for claiming, releasing, reading, and writing GPIO pins.
A claim includes the pin's pull setting.
Events from both edges can be delivered to a waiting thread.

Only one owner can claim a pin at a time.
The owner can read it as an input or drive it as an output.
This works on the emulator and every supported board.

The [hosted-runtime suite](GETTING-STARTED.md#understand-what-the-checks-cover)'s GPIO section tests the emulator configurations' virtual lines.
The hosting API check (hosting-api gate) also runs three GPIO cases and two deliberately modified test controls.

## Use I2C and SPI

The same package's `I2cDevice.Create` and `SpiDevice.Create` run unchanged.
Their Linux drivers open device files that the hosting layer maps to kernel bus operations.

| Interface | Device file | Behavior |
| --- | --- | --- |
| I2C | `/dev/i2c-<bus>` | Each message carries at most 255 bytes. A transaction supports up to eight messages, with repeated starts between them. |
| SPI | `/dev/spidev<bus>.<select>` | Each device has its own mode, clock, and chip select. Read-only operations clock out ones. |

A bus is claimed for the program's run and shared by its devices under that ownership.
Bus numbers match the chip's own block numbers, as on Linux.
The I2C message limit and the SPI read behavior differ from Linux.

Every wait for a bus transfer uses bounded polling.
No such wait runs inside the kernel's critical section.
The kernel supports the `System.Device.I2c` and `System.Device.Spi` drivers on the emulator's hardware models.
It also supports the ST and RP2350 boards whose configurations provide a bus.
A board configuration without a bus reports a bus count of zero.

The hosted-runtime suite's I2C and SPI sections test the emulator's hardware models.
The hosting-api gate adds three bus cases, run over the ST blocks' models and again over the RP2350's own drivers, and two deliberately modified test controls.

## Use PWM

The same package's `PwmChannel.Create(0, <pin>,
  frequency, dutyCycle)` runs unchanged.
Its Linux driver reads and writes device files under `/sys/class/pwm/pwmchip0`.
The hosting layer maps these accesses to the kernel's PWM operations for `System.Device.Pwm`.

The channel number is the pin's GPIO number: `PwmChannel.Create(0, 15, ...)` drives GPIO 15.
A Linux controller numbers its own outputs and binds each to a pin by configuration outside the program; here the pin a program names is the pin it gets.
`new TargetBoard().CreatePwmChannel(0, 15, frequency, dutyCycle)` opens the same channel through the board class, in the standard board class's form; a chip or channel no pin carries is refused with `NotSupportedException`.
The chip's channel count is the highest PWM-capable pin number plus one; a pin no timer drives is refused when the driver exports it.
Only one owner can claim a timer channel at a time: a pin that reaches the same timer output as a held pin is refused.

The period and duty cycle are set in nanoseconds.
The program can drive the output or park it.
A parked output rests at its inactive level; releasing the channel parks it too.

Unlike Linux, the period file reports the period the timer actually runs, including its quantization.
It does not report the requested value.
The duty-cycle file reads back the value asked, as on Linux.

The hosted-runtime suite's PWM section tests the emulator's virtual timers and includes a deliberately modified test control.
The shim is the compatibility layer between the hosted runtime and the kernel.
The shim gate's PWM checks add four deliberately modified test controls.

PWM also works on the RP2350 boards' PWM hardware, called slices.

## Use the serial port

A program's `new SerialPort("/dev/ttyS0", 115200)` from `System.IO.Ports` runs unchanged.
The package's Linux driver opens the device file.
The hosting layer maps the device file to the kernel's UART operations.
The package's native layer is compiled unchanged from its own sources into the hosting layer.

The port number is the chip's UART block number: `/dev/ttyS0` is UART0.
On the Pico boards UART0 is on GP0 and GP1 by default, and UART1 has no default pins.

The hosted-runtime suite's serial section tests the emulator's second UART.
The verification chain echoes the bytes the section sends and the section reads them back.

## Read an analog input

An analog input is read through the standard analog classes of `System.Device.Analog`, over the board package (`Lamella.Hardware`).
`new TargetBoard().CreateAnalogController(0)` returns the `AnalogController` over the chip's converter (the chip must be 0, as on the standard Arduino board),
and its `OpenPin(<pin>)` opens the input on a pin, numbered by its GPIO line: `OpenPin(26)` on a Pico 2, `OpenPin(40)` on a Pico Plus 2, whose header prints GP26 for the chip's pad 40.
`SupportsAnalogInput(<pin>)` says which pins carry an input.
`ReadRaw()` is one conversion, a count within `AdcResolutionBits`; disposing the pin releases the channel.

The pin's `ReadVoltage()`, `MinVoltage` and `MaxVoltage` compute where the runtime host configuration option `Lamella.Hardware.AnalogVoltage.IsSupported` is true,
which the SDK sets per program: true on a board whose flash holds the library the standard classes express a voltage in (the Pimoroni Pico Plus 2 boards and the Pico LiPo 2, the Pico LiPo 2 XL W; the STM32F769I-DISCO when the project property `RhuProgramMemory=external` places the program in its external flash)
when the program's own code names one of the voltage members (docs/BUILDING.md).
There the controller's `VoltageReference` is set for you to the board's reference (3.3 V), a program may set another before it opens a pin, and the publish reports one trim-analysis warning from that library, expected.
Where the option is false, the three members are not supported: each throws `NotSupportedException` naming the board, the image carries no code of the library, and a program reads counts with `ReadRaw`; a project may set the option itself.

The classes read the Linux IIO device files the hosting layer presents.
`/sys/bus/iio/devices/iio:device0/in_voltage<channel>_raw` answers one conversion per read at the start of the file,
and `in_voltage_scale` states the millivolts one count stands for; a channel that reads no pin (the chip's temperature sensor) is read through its file.

The open claims the channel for the program.
A pin a program holds as a GPIO line cannot be opened as an analog input until the line is closed,
and a pin held as an analog input cannot be opened as a GPIO line until the input is disposed.
A second open of a held pin is refused: with `InvalidOperationException` by the controller that holds it, with `IOException` by another;
a pin without the analog function is refused with `NotSupportedException`; value-changed events are not supported.
A conversion the converter does not complete within its budget, or flags in error on every retry, is refused with `IOException` saying which; the next read converts again.

## Use calendar time and secure randomness

`DateTime.UtcNow` and file times count from the .NET epoch until a board clock, debug channel, or application sets the base.
A native timed wait against the real-time clock follows the calendar, including later changes to that base.

Secure random bytes (`RandomNumberGenerator`) come from a hardware source; where the image has none, the call is refused and the boot banner says so.
Emulator images use a software stream selected by the build option `entropy-accept-software`, which their boot banner names.
The hosting-api gate tests entropy behavior.
The shim gate tests calendar behavior, timed waits, and the secure random API.

## Use double-precision arithmetic

Cortex-M33 boards have a single-precision FPU.
The kernel traps and emulates double-precision instructions.
A program's `double` arithmetic produces the same results as on Cortex-M7, with one fault per emulated instruction.

The hosting-api gate calculates the same vector set on both processor classes and compares the results.
The Cortex-M7 configuration uses its FPU; the Cortex-M33 configuration uses emulation.
