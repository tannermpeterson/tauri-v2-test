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

      <h1>Welcome to Tauri!</h1>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          greet();
        }}
      >
        <input
          id="greet-input"
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit">Greet</button>
      </form>

      <p>{greetMsg}</p>
      <button onClick={onLiveViewClick}>{liveViewBtnText}</button>
    </div>
  );
}

export default App;
