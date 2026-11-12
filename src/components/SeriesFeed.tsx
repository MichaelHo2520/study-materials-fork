import { useEffect, useRef, useState } from "react";
import Feed from "./Feed";
import { api } from "../api";
import { useStore } from "../store";
import { IBack } from "../icons";

type Props = {
  id: number;
  title: string;
  active: boolean;
  bottomInset: number;
  onBack: () => void;
  startEp?: number;
  startTime?: number;
};

/** 单部剧集播放页：先取集数表，再交给 Feed 以「一格一集」的方式播放，上下滑动只在这部的各集之间切换。 */
export default function SeriesFeed({ id, title, active, bottomInset, onBack, startEp, startTime }: Props) {
  const { history } = useStore();
  const [eps, setEps] = useState<number[] | null>(null);
  const init = useRef<{ index: number; ep: number } | null>(null);

  useEffect(() => {
    let alive = true;
    api.video(id)
      .then((d) => { if (alive) setEps((d.episodes || []).map((e) => e.ep_num).sort((a, b) => a - b)); })
      .catch(() => { if (alive) setEps([]); });
    return () => { alive = false; };
  }, [id]);

  if (!eps) {
    return (
      <div className="absolute inset-0 bg-black flex items-center justify-center">
        <button onClick={onBack} aria-label="返回" className="hg-chip absolute left-2.5 w-9 h-9 rounded-full flex items-center justify-center pressable"
          style={{ top: "calc(var(--sat, 0px) + 8px)" }}><IBack size={20} /></button>
        <div className="w-9 h-9 spinner" />
      </div>
    );
  }

  /* 取不到集数表（或只有一集）时退回普通播放页 */
  if (eps.length < 2) {
    return <Feed videoIds={[id]} title={title} mode="context" active={active} bottomInset={bottomInset} onBack={onBack} startEp={startEp} startTime={startTime} />;
  }

  if (!init.current) {
    const want = startEp || history.find((h) => h.id === id)?.ep || eps[0];
    const index = Math.max(0, eps.indexOf(want));
    init.current = { index, ep: eps[index] };
  }

  return (
    <Feed videoIds={[id]} seriesEps={eps} startIndex={init.current.index} startEp={init.current.ep} startTime={startTime}
      title={title} mode="context" active={active} bottomInset={bottomInset} onBack={onBack} />
  );
}
