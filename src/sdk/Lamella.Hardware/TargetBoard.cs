// The board class: one class for every board, built by name; the SDK generates the board's facts into the program at publish and hands them over before Main.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Device.Gpio;
using System.Device.I2c;
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
        _f = s_facts ?? throw new InvalidOperationException("this program was not published for a board by the Lamella SDK, so no TargetBoard is known");
        Pins = new BoardPins(_f.Pins);
    }

    /// <summary>The SDK's seam: the generated source hands the board's facts over once, before Main.</summary>
    [EditorBrowsable(EditorBrowsableState.Never)]
    public static void SetBuildFacts(BoardFacts facts)
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

    /// <summary>The standard controller over the board's pins.</summary>
    public GpioController CreateGpioController() => new GpioController();

    /// <summary>A device on an I2C bus at the standard rate, 100 kHz.</summary>
    public I2cDevice CreateI2cDevice(I2cConnectionSettings settings) => CreateI2cDevice(settings, StandardI2cFrequency);

    /// <summary>
    /// A device on an I2C bus at a clock frequency in hertz the image serves; a bus the board does not
    /// have or a rate the image does not serve is refused with <see cref="NotSupportedException"/>,
    /// the message naming the board, the rate asked and the image's clock posture. The bus then runs
    /// at that rate for the program's run, set by the first device created on it: a device at another
    /// rate on the same bus is refused with <see cref="InvalidOperationException"/> naming the
    /// running rate, and a device created the standard way runs the bus at 100 kHz.
    /// </summary>
    public I2cDevice CreateI2cDevice(I2cConnectionSettings settings, int clockFrequency)
    {
        ArgumentNullException.ThrowIfNull(settings);
        I2cBusInfo bus = FindI2c(settings.BusId);
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

        ClaimRate(bus, clockFrequency);
        return I2cDevice.Create(settings);
    }

    /// <summary>
    /// A device on an SPI bus; the clock number is passed to the image as is and served at the
    /// nearest rate the block's ladder makes, as a Raspberry Pi serves it. A bus the board does
    /// not have is refused with <see cref="NotSupportedException"/>.
    /// </summary>
    public SpiDevice CreateSpiDevice(SpiConnectionSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        FindSpi(settings.BusId);
        return SpiDevice.Create(settings);
    }

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
