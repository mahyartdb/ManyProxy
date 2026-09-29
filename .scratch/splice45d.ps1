$m = Get-Content src/main.rs
$mh = Get-Content $env:TEMP/mHEAD2.rs
$m45 = Get-Content $env:TEMP/m45.rs
function Rep($arr, $from, $to, $new) {
  $out = @()
  if ($from -gt 1) { $out += $arr[0..($from - 2)] }
  $out += $new
  if ($to -lt $arr.Count) { $out += $arr[$to..($arr.Count - 1)] }
  return $out
}
function Slice($arr, $from, $to) { return @($arr[($from - 1)..($to - 1)]) }
function FindFn($arr, $name, $from) {
  for ($i = $from; $i -lt $arr.Count; $i++) {
    if ($arr[$i] -match ('^\s*fn ' + $name + '\(')) { return $i }
  }
  return -1
}
function FnEnd($arr, $start) {
  for ($i = $start + 1; $i -lt $arr.Count; $i++) {
    if ($arr[$i] -match '^\}$|^    \}$') { return $i }
  }
  return -1
}
$ls = FindFn $mh 'library_row_lines' 0; $le = FnEnd $mh $ls
Write-Host ('fn43@' + $ls + '-' + $le)
$fn43 = @($mh[($ls - 2)..$le])
$vl45s = FindFn $m45 'visible_log_lines' 0; $vl45e = FnEnd $m45 $vl45s
$fn45 = @($m45[($vl45s - 4)..$vl45e])
$w45s = FindFn $m45 'write_visible_logs_to' 0; $w45e = FnEnd $m45 $w45s
$sv45s = FindFn $m45 'save_visible_logs' 0; $sv45e = FnEnd $m45 $sv45s
$cpHs = FindFn $mh 'copy_visible_logs' 0
$copyDoc = @($mh[($cpHs - 3)..($cpHs - 1)])
$copyNew = $copyDoc + @(
  '    fn copy_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {',
  '        let lines = self.visible_log_lines(cx);',
  '        if lines.is_empty() {',
  '            window.push_notification(Notification::warning("No log lines to copy"), cx);',
  '            return;',
  '        }'
)
$lh = FindFn $mh 'log_lines' 0
$lhDoc = @($mh[($lh - 2)..($lh - 1)])
  '    fn log_lines(&self, cx: &App) -> Vec<String> {',
  '        self.visible_log_lines(cx)',
  '    }'
))
$m = Rep $m 4967 4971 (Slice $m 4968 4968)
$m = Rep $m 5061 5111 ($lhDoc + @('    fn log_lines(&self, cx: &App) -> Vec<String> {','        self.visible_log_lines(cx)','    }'))
$m = Rep $m 4940 4953 ((Slice $m 4941 4945) + (Slice $m 4948 4952))
$m = Rep $m 2718 2729 ((Slice $m 2719 2723) + (Slice $m 2726 2728))
$m = Rep $m 1613 1621 ((Slice $m 1614 1615) + (Slice $m 1617 1620))
$ws = FindFn $m 'library_row_lines_with_upstream' 0
Write-Host ('withsig@' + $ws + ' line=[' + $m[$ws + 4] + ']')
$ins = @()
for ($i = 0; $i -lt $m.Count; $i++) { $ins += $m[$i]; if ($i -eq ($ws + 4)) { $ins += '    traffic: Option<(u64, u64)>,' } }
$m = $ins
$m = Rep $m 1563 1581 ((Slice $m 1564 1565) + $fn43)
$m = Rep $m 498 510 ((Slice $m 499 500) + (Slice $m 504 508) + @('        for line in library_row_lines_with_upstream(&row, &upstream, health, is_running, traffic) {'))
$m = Rep $m 346 370 @(
  '        let lowered = self.query.trim().to_lowercase();',
  '        self.visible = self',
  '            .search_index',
  '            .iter()',
  '            .enumerate()',
  '            .filter(|(_, blob)| blob.matches_lowered(&lowered))',
  '            .map(|(index, _)| index)',
  '            .collect();'
)
$m = Rep $m 37 43 @(
  'use health::{',
  '    ping_all, redact_secrets, sparkline_latencies, test_all, truncate, HealthProber, HealthStore,',
  '    LogLevel, LogStore, LogTagIndex, RealHealthProber, TestOutcome, TrafficSeenMap, DEFAULT_TEST_URL,',
  '};'
)
$te = FnEnd $m $ts
$m = Rep $m ($ts - 2) ($te + 2) @()
Set-Content -Path src/main.rs -Value $m
Write-Host ('written lines=' + (Get-Content src/main.rs).Count)
Select-String -Path src/main.rs -Pattern '^(<<<<<<<|=======|>>>>>>>)' | Select-Object -First 3
Write-Host 'markers-done(empty=clean)'
