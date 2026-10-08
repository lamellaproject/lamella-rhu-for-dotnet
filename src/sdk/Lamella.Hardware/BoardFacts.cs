// The facts the SDK generates into a program at publish, handed to TargetBoard before Main.
using System.Collections.Generic;

namespace Lamella.Hardware;

/// <summary>
/// The board's facts as the SDK generated them from the board table's row: the SDK's seam,
/// not a program's. <paramref name="Posture"/> names the image's clock posture in words
/// ("the PLL posture") for the sentences the class prints.
/// </summary>
public sealed record BoardFacts(
    string Name,
    string Maker,
    string Product,
    string Chip,
    int ClockFrequency,
    string Posture,
    IReadOnlyList<BoardPin> Pins,
    IReadOnlyList<I2cBusInfo> I2cBuses,
    IReadOnlyList<SpiBusInfo> SpiBuses);
