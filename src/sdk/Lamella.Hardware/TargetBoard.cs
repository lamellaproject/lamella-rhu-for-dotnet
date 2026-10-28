// The board class: one class for every board, built by name; the SDK generates the board's facts into the program at publish and hands them over before Main.
// The class answers them and refuses, with Microsoft's own exception class
// for "this board cannot do what was asked", a bus the board does not have or
// an I2C rate the image does not serve. The rate a program asks for reaches
// the image through RHU's one request on the I2C device node (Linux has no
// such setting; the bus then runs at that rate for the program's run),
// issued through the three C-library calls the standard package itself binds.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Device.Analog;
using System.Device.Gpio;
using System.Device.Gpio.Drivers;
using System.Device.I2c;
using System.Device.Pwm;
using System.Device.Spi;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Lamella.Hardware;

/// <summary>The board this program was built for.</summary>
public sealed class TargetBoard
{
    /// <summary>The I2C clock frequency every bus serves: the standard rate, 100 kHz.</summary>
    public const int StandardI2cFrequency = 100_000;

    private static BoardFacts? s_facts;

    private readonly BoardFacts _f;

    /// <summary>The board the program was built for; cheap, and the same facts every time.</summary>
    public TargetBoard()
    {
        _f = s_facts ?? throw new InvalidOperationException("no board: this program was not published for a board (LamellaTargetBoard names one; dotnet publish or dotnet run --device builds the image for it), so no TargetBoard is known");
        Pins = new BoardPins(_f.Pins);
    }

    /// <summary>The SDK's seam: the generated source hands the board's facts over once, before Main.</summary>
    [EditorBrowsable(EditorBrowsableState.Never)]
    internal static void SetBuildFacts(BoardFacts facts)
    {
        ArgumentNullException.ThrowIfNull(facts);
        if (s_facts is not null)
        {
            throw new InvalidOperationException("the board's facts were already set for this program (the SDK sets them once, at publish)");
        }

        s_facts = facts;
    }

    /// <summary>The board's id, the value the build named: "rpi-pico2-w".</summary>
    public string Name => _f.Name;

    /// <summary>The board's maker, as printed: "Raspberry Pi".</summary>
    public string Maker => _f.Maker;

    /// <summary>The board's product name, as printed: "Pico 2 W".</summary>
    public string Product => _f.Product;

    /// <summary>The chip: "RP2350A".</summary>
    public string Chip => _f.Chip;

    /// <summary>The image's clock frequency in hertz (its clock posture).</summary>
    public int ClockFrequency => _f.ClockFrequency;

    /// <summary>The pins of the chip's package, by number and by the labels printed on the board.</summary>
    public BoardPins Pins { get; }

    /// <summary>The I2C buses the image serves, with their rates.</summary>
    public IReadOnlyList<I2cBusInfo> I2cBuses => _f.I2cBuses;

    /// <summary>The SPI buses the image serves, with their rate ladders.</summary>
    public IReadOnlyList<SpiBusInfo> SpiBuses => _f.SpiBuses;

    /// <summary>The width in bits of a conversion on the chip's converter; 0 where the image serves no analog input.</summary>
    internal int AnalogResolutionBits => _f.AnalogResolutionBits;

    /// <summary>The voltage a full-scale conversion stands for, in millivolts; 0 where the image serves no analog input.</summary>
    internal int AnalogReferenceMillivolts => _f.AnalogReferenceMillivolts;

    /// <summary>One line: the board, the chip, the clock, the pins, the buses.</summary>
    public override string ToString()
    {
        var b = new StringBuilder();
        b.Append(Maker).Append(' ').Append(Product).Append(" (").Append(Name).Append("), ");
        b.Append(Chip).Append(" at ").Append(ClockFrequency).Append(" Hz, ");
        b.Append(Pins.Count).Append(" pins, I2C ");
        AppendNumbers(b, I2cBuses.Count, i => I2cBuses[i].Number);
        b.Append(", SPI ");
        AppendNumbers(b, SpiBuses.Count, i => SpiBuses[i].Number);
        return b.ToString();
    }

    /// <summary>
    /// The standard controller over the board's pins, on the one driver the kernel serves: libgpiod's
    /// first chip, named here so the published image carries that driver alone and none of the
    /// standard class's board detection.
    /// </summary>
    public GpioController CreateGpioController() => new GpioController(new LibGpiodDriver(0));

    /// <summary>A device on an I2C bus at the standard rate, 100 kHz.</summary>
    public I2cDevice CreateI2cDevice(I2cConnectionSettings connectionSettings) => CreateI2cDevice(connectionSettings, StandardI2cFrequency);

    /// <summary>
    /// A device on an I2C bus at a clock frequency in hertz the image serves; a bus the board does not
    /// have or a rate the image does not serve is refused with <see cref="NotSupportedException"/>,
    /// the message naming the board, the rate asked and the image's clock posture. The bus then runs
    /// at that rate for the program's run, set by the first device created on it: a device at another
    /// rate on the same bus is refused with <see cref="InvalidOperationException"/> naming the
    /// running rate, and a device created the standard way runs the bus at 100 kHz.
    /// </summary>
    public I2cDevice CreateI2cDevice(I2cConnectionSettings connectionSettings, int clockFrequency)
    {
        ArgumentNullException.ThrowIfNull(connectionSettings);
        I2cBusInfo bus = FindI2c(connectionSettings.BusId);
        bool served = false;
        foreach (int r in bus.Frequencies)
        {
            if (r == clockFrequency)
            {
                served = true;
                break;
            }
        }

        if (!served)
        {
            var b = new StringBuilder();
            b.Append(Board()).Append(": an I2C rate of ").Append(clockFrequency).Append(" Hz on bus ").Append(bus.Number);
            b.Append(" is not served by this image. The image runs the chip at ").Append(ClockFrequency).Append(" Hz (").Append(_f.Posture).Append(") and serves ");
            AppendNumbers(b, bus.Frequencies.Count, i => bus.Frequencies[i]);
            b.Append(" Hz.");
            throw new NotSupportedException(b.ToString());
        }

        I2cDevice device = I2cDevice.Create(connectionSettings);
        try
        {
            ClaimRate(bus, clockFrequency);
        }
        catch
        {
            device.Dispose();
            throw;
        }

        return device;
    }

    /// <summary>
    /// A device on an SPI bus; the clock number is passed to the image as is and served at the
    /// nearest rate the block's ladder makes, as a Raspberry Pi serves it. A bus the board does
    /// not have is refused with <see cref="NotSupportedException"/>.
    /// </summary>
    public SpiDevice CreateSpiDevice(SpiConnectionSettings connectionSettings)
    {
        ArgumentNullException.ThrowIfNull(connectionSettings);
        FindSpi(connectionSettings.BusId);
        return SpiDevice.Create(connectionSettings);
    }

    /// <summary>
    /// The standard PWM channel at a chip's channel, the standard board class's form: <c>PwmChannel.Create</c> with
    /// the two numbers, at a frequency in hertz and a duty cycle from 0 to 1; the channel starts stopped, as the
    /// standard class leaves it. On this system the chip is 0 and the channel number is the GPIO line the channel
    /// drives, the two numbers a pin's record carries (<see cref="BoardPin.PwmChip"/>,
    /// <see cref="BoardPin.PwmChannelNumber"/>); a chip and channel no pin's record carries is refused with
    /// <see cref="NotSupportedException"/> naming the board.
    /// </summary>
    public PwmChannel CreatePwmChannel(int chip, int channel, int frequency = 400, double dutyCyclePercentage = 0.5)
    {
        FindPwm(chip, channel);
        return PwmChannel.Create(chip, channel, frequency, dutyCyclePercentage);
    }

    /// <summary>
    /// The standard PWM channel at a chip's channel on a named pin, the standard board class's second form: on this
    /// system a channel drives the one line its number names, so a pin other than that line is refused with
    /// <see cref="NotSupportedException"/> naming both.
    /// </summary>
    public PwmChannel CreatePwmChannel(int chip, int channel, int frequency, double dutyCyclePercentage, int pin)
    {
        BoardPin record = FindPwm(chip, channel);
        if (pin != record.PinNumber)
        {
            throw new NotSupportedException(Board() + ": PWM chip " + chip + ", channel " + channel + " drives " + Label(record) + " alone on this board, so it cannot be created on pin " + pin + ".");
        }

        return PwmChannel.Create(chip, channel, frequency, dutyCyclePercentage);
    }

    private BoardPin FindPwm(int chip, int channel)
    {
        foreach (BoardPin pin in Pins)
        {
            if (pin.PwmChip == chip && pin.PwmChannelNumber == channel)
            {
                return pin;
            }
        }

        throw new NotSupportedException(Board() + ": no pin carries PWM chip " + chip + ", channel " + channel + " on this board, so no PWM channel can be created there.");
    }

    /// <summary>
    /// The standard analog controller over the chip's converter, the standard Arduino board's form: chip must be 0,
    /// the one converter the image presents; another number, or a board whose image serves no analog input, is
    /// refused with <see cref="NotSupportedException"/> naming the board. The controller numbers its pins by GPIO
    /// line, the numbers <c>GpioController.OpenPin</c> takes: <c>SupportsAnalogInput</c> says which lines carry an
    /// analog input, <c>OpenPin</c> claims the line's channel for this program (a line a program holds as a GPIO
    /// line, or a channel a controller holds, is refused), and the pin's <c>ReadRaw</c> is one conversion as a count
    /// within <c>AdcResolutionBits</c>. The voltage members compute where <see cref="AnalogVoltage.IsSupported"/> is
    /// true, with <c>VoltageReference</c> set to the board's reference (a program may set another before it opens a
    /// pin), and throw <see cref="NotSupportedException"/> where it is false, with the reference unset.
    /// </summary>
    public AnalogController CreateAnalogController(int chip)
    {
        if (_f.AnalogResolutionBits == 0)
        {
            throw new NotSupportedException(Board() + ": this board's image serves no analog input.");
        }

        if (chip != 0)
        {
            throw new NotSupportedException(Board() + " has no analog controller " + chip + "; its one controller is chip 0.");
        }

        return new IioAnalogController(this);
    }

    internal string BoardWords() => Board();

    internal string PinWords(int pinNumber) => Label(Pins[pinNumber]);

    // RHU's one request on the I2C device node: the bus's rate in hertz, one 32-bit word in and
    // out (the shim's BUSDEV_I2C_RATE; the harness ties the two numbers). A Linux board's bus
    // speed is a device-tree property fixed for the boot, so the standard package has no setting
    // for it; here the rate is set once for the program's run, by the first device created on the
    // bus, through the three C-library calls the standard package itself binds.
    private const uint BusRateRequest = 0x0740;
    private const int OpenReadWrite = 2;
    private const int ErrnoBusy = 16;
    private const int ErrnoNotTty = 25;
    private const int ErrnoNotSupported = 95;

    private void ClaimRate(I2cBusInfo bus, int clockFrequency)
    {
        string node = "/dev/i2c-" + bus.Number;
        int fd = Native.open(node, OpenReadWrite);
        if (fd < 0)
        {
            throw new IOException(NodeNotOpened(node, Marshal.GetLastPInvokeError()));
        }

        try
        {
            IntPtr word = Marshal.AllocHGlobal(4);
            try
            {
                Marshal.WriteInt32(word, clockFrequency);
                if (Native.ioctl(fd, BusRateRequest, word) == 0)
                {
                    return;
                }

                int errno = Marshal.GetLastPInvokeError();
                int running = Marshal.ReadInt32(word);
                switch (errno)
                {
                    case ErrnoBusy when running != clockFrequency:
                        throw new InvalidOperationException(BusAlreadyRuns(bus, running, clockFrequency));
                    case ErrnoBusy:
                        throw new InvalidOperationException(BusPinHeld(bus));
                    case ErrnoNotSupported:
                        throw new NotSupportedException(RateRefusedByImage(bus, clockFrequency));
                    case ErrnoNotTty when clockFrequency == StandardI2cFrequency:
                        return;
                    case ErrnoNotTty:
                        throw new NotSupportedException(NoRateRequest(bus, clockFrequency));
                    default:
                        throw new IOException(RequestRefused(node, errno));
                }
            }
            finally
            {
                Marshal.FreeHGlobal(word);
            }
        }
        finally
        {
            Native.close(fd);
        }
    }

    private string BusAlreadyRuns(I2cBusInfo bus, int running, int asked) =>
        Board() + ": I2C bus " + bus.Number + " already runs at " + running + " Hz for this program's run; a bus has one rate per run, set by the first device created on it, and " + asked + " Hz was asked.";

    private string BusPinHeld(I2cBusInfo bus) =>
        Board() + ": I2C bus " + bus.Number + " could not be claimed: a program holds one of its pins (" + Label(bus.Sda) + ", " + Label(bus.Scl) + ") as a GPIO line.";

    private string RateRefusedByImage(I2cBusInfo bus, int asked) =>
        Board() + ": an I2C rate of " + asked + " Hz on bus " + bus.Number + " is listed by the board's facts but refused by the image (the image's clock does not serve it).";

    private string NoRateRequest(I2cBusInfo bus, int asked) =>
        Board() + ": this image's I2C node takes no clock-frequency request, so bus " + bus.Number + " runs at its default rate and " + asked + " Hz cannot be set.";

    private string NodeNotOpened(string node, int errno) =>
        "the I2C node " + node + " of " + Board() + " could not be opened (errno " + errno + ")";

    private string RequestRefused(string node, int errno) =>
        "the I2C node " + node + " of " + Board() + " refused the rate request (errno " + errno + ")";

    private static string Label(BoardPin pin) => pin.Labels.Count > 0 ? pin.Labels[0] : "pin " + pin.PinNumber;

    // The standard package's own binding of the three calls (its Interop.Libc at the pin: the
    // library "libc", SetLastError, the path as a C string, the request a 32-bit word, the
    // argument a pointer); the SDK binds them directly at link time.
    private static class Native
    {
        [DllImport("libc", SetLastError = true)]
        internal static extern int open([MarshalAs(UnmanagedType.LPStr)] string pathname, int flags);

        [DllImport("libc", SetLastError = true)]
        internal static extern int ioctl(int fd, uint request, IntPtr argp);

        [DllImport("libc")]
        internal static extern int close(int fd);
    }

    private string Board() => Maker + " " + Product + " (" + Name + ")";

    private I2cBusInfo FindI2c(int number)
    {
        foreach (I2cBusInfo bus in I2cBuses)
        {
            if (bus.Number == number)
            {
                return bus;
            }
        }

        throw new NotSupportedException(Missing("I2C", number, I2cBuses.Count, i => I2cBuses[i].Number));
    }

    private SpiBusInfo FindSpi(int number)
    {
        foreach (SpiBusInfo bus in SpiBuses)
        {
            if (bus.Number == number)
            {
                return bus;
            }
        }

        throw new NotSupportedException(Missing("SPI", number, SpiBuses.Count, i => SpiBuses[i].Number));
    }

    private string Missing(string kind, int number, int count, Func<int, int> at)
    {
        var b = new StringBuilder();
        b.Append(Board()).Append(" has no ").Append(kind).Append(" bus ").Append(number).Append("; ");
        if (count == 0)
        {
            b.Append("the row carries no ").Append(kind).Append(" bus");
        }
        else
        {
            b.Append("its buses: ");
            AppendNumbers(b, count, at);
        }

        return b.ToString();
    }

    private static void AppendNumbers(StringBuilder b, int count, Func<int, int> at)
    {
        if (count == 0)
        {
            b.Append("none");
            return;
        }

        for (int i = 0; i < count; i++)
        {
            if (i > 0)
            {
                b.Append(", ");
            }

            b.Append(at(i));
        }
    }
}
