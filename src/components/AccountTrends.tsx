import { useMemo } from "react";
import type { AccountInfo, BalanceSnapshot } from "../types";
import { balanceSnapshotCount, snapshotMetricCount, totalSnapshotCount } from "../drawerSections";
import DrawerSection from "./DrawerSection";
import TrendRangeControls, { useTrendRange } from "./TrendRangeControls";
import { LazyBalanceTrendChart as BalanceTrendChart } from "./charts/lazy";

interface Props {
  accountId: string;
  snapshots: BalanceSnapshot[];
  info: AccountInfo | null;
  now?: number;
  balanceEmptyNote?: string;
  showUsedTrend?: boolean;
}

export default function AccountTrends({ accountId, snapshots, info, now, balanceEmptyNote, showUsedTrend = false }: Props) {
  const currentCurrency = info?.balance?.currency ?? (showUsedTrend ? info?.used?.currency : undefined);
  const own = useMemo(() => {
    const account = snapshots.filter((s) => s.accountId === accountId &&
      (Number.isFinite(s.balance) || (showUsedTrend && Number.isFinite(s.used))));
    const currency = currentCurrency ?? account[account.length - 1]?.currency;
    return account.filter((s) => s.currency === currency);
  }, [snapshots, accountId, currentCurrency, showUsedTrend]);
  const range = useTrendRange(own, now);
  const balanceCount = balanceSnapshotCount(range.filtered, accountId);
  const usedCount = showUsedTrend ? snapshotMetricCount(range.filtered, accountId, "used") : 0;
  const totalCount = totalSnapshotCount(range.filtered, accountId);
  const selected = range.selection.preset !== "all";
  const balanceNote = range.window.error ?? (balanceCount === 1
    ? "已有 1 个快照，再刷新一次即可成趋势"
    : selected ? "此时间范围暂无余额记录，可选择更长时间或“全部”" : balanceEmptyNote);
  const usedNote = range.window.error ?? (usedCount === 1
    ? "已有 1 个累计已用快照，再刷新一次即可成趋势"
    : selected ? "此时间范围暂无累计已用记录，可选择更长时间或“全部”"
      : "暂无累计已用记录，刷新后开始记录");
  const totalNote = range.window.error ?? (totalCount === 1
    ? "已有 1 个总额度快照，再成功刷新一次即可成趋势"
    : selected ? "此时间范围暂无总额度记录，可选择更长时间或“全部”"
      : info && info.total == null ? "该接口未提供总额度，暂无总额度历史"
        : "总额度从成功刷新后开始记录，已有余额历史不补填总额度");
  return (
    <div className="account-trends">
      <TrendRangeControls selection={range.selection} onChange={range.setSelection} bounds={range.bounds} window={range.window} />
      <DrawerSection key={`balance-${balanceCount >= 2}`} title="余额趋势" hasData={balanceCount >= 2} emptyNote={balanceNote}>
        <BalanceTrendChart snapshots={range.filtered} accountId={accountId} window={range.window} emptyNote={balanceNote} />
      </DrawerSection>
      {showUsedTrend && (
        <DrawerSection key={`used-${usedCount >= 2}`} title="累计已用趋势" hasData={usedCount >= 2} emptyNote={usedNote}>
          <BalanceTrendChart snapshots={range.filtered} accountId={accountId} metric="used" window={range.window} emptyNote={usedNote} />
        </DrawerSection>
      )}
      <DrawerSection key={`total-${totalCount >= 2}`} title="总额度趋势" hasData={totalCount >= 2} emptyNote={totalNote}>
        <BalanceTrendChart snapshots={range.filtered} accountId={accountId} metric="total" window={range.window} emptyNote={totalNote} />
      </DrawerSection>
      <p className="trend-history-note">本地刷新快照，不代表每笔扣费时间；超过 6 小时未采样处不连线。历史按账号保留最近 90 天，最多 15 万条。</p>
    </div>
  );
}
