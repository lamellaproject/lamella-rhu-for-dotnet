// The switch the analog classes' voltage members read: whether this program's row carries the library the standard
// analog classes express a voltage in. The SDK sets it per program through the runtime host configuration option of
// the same name, trimmed (true where the board's row holds the library and the program's own code names a voltage
// member, false otherwise; a project may set the option itself), so the compiler keeps one branch of each member:
// true, the members compute and the library is carried; false, they throw and the image carries no code of the
// library. An absent option reads false here at run time, but the compiler then keeps both branches.
using System;
using System.Diagnostics.CodeAnalysis;

namespace Lamella.Hardware;

/// <summary>
/// Whether the analog classes' voltage members compute on this board. <see cref="IsSupported"/> reads the runtime host
/// configuration option <c>Lamella.Hardware.AnalogVoltage.IsSupported</c> (absent, false): true, the library the
/// standard analog classes express a voltage in is carried and <c>ReadVoltage</c>, <c>MinVoltage</c> and
/// <c>MaxVoltage</c> compute; false, the three throw <see cref="NotSupportedException"/> and the image carries no code
/// of that library. <c>ReadRaw</c> and <c>AdcResolutionBits</c> work either way.
/// </summary>
public static class AnalogVoltage
{
    private const string Switch = "Lamella.Hardware.AnalogVoltage.IsSupported";

    /// <summary>True when the voltage members compute on this board; false when they throw and the image carries no code of the library.</summary>
    [FeatureSwitchDefinition(Switch)]
    public static bool IsSupported { get; } = AppContext.TryGetSwitch(Switch, out bool supported) && supported;
}
