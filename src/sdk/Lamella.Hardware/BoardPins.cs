// The board's pin table: one record per pin of the chip's package, in pin-number order.
using System;
using System.Collections;
using System.Collections.Generic;

namespace Lamella.Hardware;

/// <summary>The board's pins, by number and by the labels the board prints.</summary>
public sealed class BoardPins : IReadOnlyList<BoardPin>
{
    private readonly BoardPin[] _pins;

    public BoardPins(IReadOnlyList<BoardPin> pins)
    {
        ArgumentNullException.ThrowIfNull(pins);
        _pins = new BoardPin[pins.Count];
        for (int i = 0; i < pins.Count; i++)
        {
            BoardPin p = pins[i] ?? throw new ArgumentException("a pin record is null", nameof(pins));
            if (p.PinNumber != i)
            {
                throw new ArgumentException($"the pin records are not in pin-number order: record {i} carries pin {p.PinNumber}", nameof(pins));
            }

            _pins[i] = p;
        }
    }

    /// <summary>The number of pins on the chip's package.</summary>
    public int Count => _pins.Length;

    /// <summary>The record of a pin by its number; a number outside the package is out of range.</summary>
    public BoardPin this[int pinNumber]
    {
        get
        {
            if (pinNumber < 0 || pinNumber >= _pins.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(pinNumber), pinNumber, $"the board has pins 0 to {_pins.Length - 1}");
            }

            return _pins[pinNumber];
        }
    }

    /// <summary>
    /// The record of a pin by a label the board prints for it ("GP25"); null when no pin carries
    /// that label on this board.
    /// </summary>
    public BoardPin? this[string label]
    {
        get
        {
            ArgumentNullException.ThrowIfNull(label);
            foreach (BoardPin p in _pins)
            {
                foreach (string l in p.Labels)
                {
                    if (string.Equals(l, label, StringComparison.Ordinal))
                    {
                        return p;
                    }
                }
            }

            return null;
        }
    }

    public IEnumerator<BoardPin> GetEnumerator() => ((IEnumerable<BoardPin>)_pins).GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => _pins.GetEnumerator();
}
