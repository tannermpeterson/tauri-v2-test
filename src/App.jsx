import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

async function startLiveView() {
  await invoke("start_live_view");
}

async function stopLiveView() {
  await invoke("stop_live_view");
}


function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");
  const [liveViewRunning, setLiveViewRunning] = useState(false);

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
    setGreetMsg(await invoke("greet", { name }));
  }

  function onLiveViewClick() {
    if (liveViewRunning) {
      stopLiveView();
      // TODO reset the threshold values here
    } else {
      startLiveView();
    }
    setLiveViewRunning(!liveViewRunning);
  }

  const liveViewBtnText = liveViewRunning ? "Stop Live View" : "Start Live View";

  return (
    <div className="container">

      <div className="row" style={{height: "300px"}}>
      </div>

      <button onClick={onLiveViewClick}>{liveViewBtnText}</button>

        <div class="row">
          <h2>Min:</h2>
          <input
            id="min-video-threshold"
            onChange={(e) => invoke("set_min_threshold", { newMinThreshold: parseInt(e.currentTarget.value)})}
            placeholder="0"
          />
        </div>
        <div class="row">
          <h2>Max:</h2>
          <input
            id="max-video-threshold"
            onChange={(e) => invoke("set_max_threshold", { newMaxThreshold: parseInt(e.currentTarget.value)})}
            placeholder="100"
          />
        </div>
    </div>
  );
}

export default App;
