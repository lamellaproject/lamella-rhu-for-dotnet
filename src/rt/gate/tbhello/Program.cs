// The board class's program: it prints the board it was built for, in five lines the gate reads in order.

using System;
using System.Device.I2c;
using Lamella.Hardware;

namespace Rhu.Hosted;

internal static class Program
{
    private static int Main()
    {
        var board = new TargetBoard();
        Console.WriteLine("TBHELLO: " + board.ToString());
        BoardPin first = board.Pins[0];
        Console.WriteLine("TBHELLO: pins " + board.Pins.Count + "; pin 0: labels " + (first.Labels.Count == 0 ? "none" : string.Join(" ", first.Labels)));
        Console.WriteLine("TBHELLO: pin 0: PWM " + (first.PwmChip is int chip ? "chip " + chip + ", channel " + first.PwmChannelNumber : "none"));
        try
        {
            using I2cDevice device = board.CreateI2cDevice(new I2cConnectionSettings(0, 0x50));
            Console.WriteLine("TBHELLO: I2C bus 0 at " + TargetBoard.StandardI2cFrequency + " Hz: device created");
        }
        catch (NotSupportedException e)
        {
            Console.WriteLine("TBHELLO: NotSupportedException: " + e.Message);
        }

        Console.WriteLine("TBHELLO: end");
        return 0;
    }
}
