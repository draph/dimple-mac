import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Session = { version: string };

export default function App() {
  const [status, setStatus] = useState("正在连接原生服务…");
  useEffect(() => {
    let active = true;
    invoke<Session>("get_session").then(
      ({ version }) => active && setStatus(`原生服务已连接 · v${version}`),
      () => active && setStatus("原生服务连接失败，请重新启动应用。"),
    );
    return () => { active = false; };
  }, []);
  return (
    <main className="mx-auto flex min-h-screen max-w-lg flex-col gap-8 px-8 py-10">
      <header>
        <p className="eyebrow">FOLDERIFY DESKTOP</p>
        <h1>让文件夹更好认</h1>
        <p className="muted">选中文件夹，挑一个图标，然后应用。</p>
      </header>
      <section className="empty" aria-labelledby="target-heading">
        <span aria-hidden="true" className="folder">▱</span>
        <h2 id="target-heading">工程原型已启动</h2>
        <p className="muted">目录选择、图标预览和写入将在后续任务中接入。</p>
      </section>
      <p role="status" className="muted text-sm">{status}</p>
    </main>
  );
}
