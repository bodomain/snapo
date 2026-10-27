from plot import activity_chart


def test_daily_chart_scales_sessions_and_preserves_literal_activity_names():
    chart = activity_chart([
        ('2026-10-15', '[red]Study', 20),
        ('2026-10-15', '[red]Study', 10),
        ('2026-10-15', 'Reading', 30),
        ('2026-10-14', 'Reading', 15),
    ], 60)
    lines = chart.plain.splitlines()
    bars = [line for line in lines if line.startswith('2026-')]
    assert bars[0].startswith('2026-10-15')
    assert bars[0].endswith('60.00m')
    assert '│' in bars[0]
    assert all(len(line) <= 60 for line in bars)
    assert bars[0].count('█') > bars[1].count('█')
    assert '[red]Study' in chart.plain
    assert any(span.style == 'red' for span in chart.spans)


def test_chart_handles_empty_zero_and_dense_days():
    assert 'No activity' in activity_chart([], 40).plain
    assert 'No activity' in activity_chart([('2026-10-15', 'Zero', 0)], 40).plain
    chart = activity_chart([('2026-10-15', 'Focus', 1)] * 100, 40)
    bar = next(line for line in chart.plain.splitlines() if line.startswith('2026-'))
    assert len(bar) <= 40
    assert bar.endswith('100.00m')
