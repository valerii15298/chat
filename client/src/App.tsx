import { createSignal, type Component, onMount } from 'solid-js';
import { createStore } from "solid-js/store";

const user_id_parsed = parseInt(localStorage.getItem("user_id")!);
const user_id = (!isNaN(user_id_parsed) && typeof user_id_parsed === "number") ? user_id_parsed : null;

const App: Component = () => {
  const [username, setUsername] = createSignal("");

  const [userId, setUserId] = createSignal(user_id ?? null);


  return <>
    {typeof (userId()) === "number" ? <Chat user_id={userId()!} /> : (
      <form>
        <label>Enter username: <input value={username()} onInput={e => setUsername(e.target.value)} /></label>
        <button onClick={(e) => {
          e.preventDefault();
          const user_name = username();
          if (!user_name.length || user_name.includes('\n')) return;
          fetch(`http://127.0.0.1:3005/register/${user_name}`, {
            method: "POST", headers: {
              Authorization: `Bearer ${userId()}`
            }
          }).then(r => r.json()).then((r: { user_id: number }) => {
            setUserId(r.user_id)
            // localStorage.setItem("user_id", r.user_id.toString());
          });
        }}>Continue</button>
      </form>
    )}
  </>;
};

type Message = {
  user_id: number;
  message: string
}

type Room = {
  name: string;
  messages: Message[];
  users: number[]
}

type User = {
  name: string,
  id: number
}

type State = {
  users: Record<number, User>
  rooms: Record<string, Room>
}

const Chat: Component<{ user_id: number }> = ({ user_id }) => {
  let ws: WebSocket;
  const [store, setStore] = createStore<State>({ rooms: {}, users: {} });
  const [wsIsReady, setWsIsReady] = createSignal(false);
  const [selectedRoom, setSelectedRoom] = createSignal<string>();
  const [newRoomName, setNewRoomName] = createSignal<string>("");
  const [msg, setMsg] = createSignal<string>("");
  async function refreshRooms() {
    await fetch('http://127.0.0.1:3005/rooms', {
      headers: {
        Authorization: `Bearer ${user_id}`
      }
    }).then(r => r.json()).then(rooms => {
      setStore("rooms", rooms);
    });
  }
  async function refreshUsers() {
    await fetch('http://127.0.0.1:3005/users', {
      headers: {
        Authorization: `Bearer ${user_id}`
      }
    }).then(r => r.json()).then(users => {
      setStore("users", users);
    });
  }
  onMount(async () => {
    await refreshRooms();
    await refreshUsers();

    ws = new WebSocket(`ws://127.0.0.1:3005/${user_id}`);
    ws.onopen = function () {
      console.log("connection opened");
      setWsIsReady(true);
    };

    ws.onclose = function () {
      console.log("connection closed");
      setWsIsReady(false);
    };

    ws.onmessage = function (event) {
      if (typeof event.data !== "string") return;
      console.log(event.data);
      let [user_id, room_name, message] = event.data.split('\n');
      if (!(user_id in store.users)) {
        refreshUsers();
      }
      setStore("rooms", room_name, "messages", messages => [...messages, { user_id: parseInt(user_id), message }])

      console.log({
        user_id, room_name, message
      })
    }
  });
  return <div>
    {Object.values(store.rooms).map(r => <button onClick={() => setSelectedRoom(r.name)}>{r.name}</button>)}
    <label>New room name: <input value={newRoomName()} onInput={e => setNewRoomName(e.target.value)} type="text" /></label>
    <button onClick={async () => {
      await fetch(`http://127.0.0.1:3005/rooms/${newRoomName()}`, {
        headers: {
          Authorization: `Bearer ${user_id}`,
        },
        method: "POST"
      }).then(r => r.text()).then(console.log)
      await refreshRooms();

    }}>Add rom</button>
    {selectedRoom() && <div>
      <div style={{
        "min-height": "100px",
        "background-color": "olivedrab",
        display: "flex",
        "flex-direction": "column",
        color: "white"
      }}>{
          store.rooms[selectedRoom()!].messages.map(m =>
            <div>{store.users[m.user_id]?.name || "Unknown user"}: {m.message}</div>)
        }
        {!store.rooms[selectedRoom()!].messages.length && <div style={{ "text-align": "center", margin: "auto" }}>No messages in this room yet</div>}
      </div>
      <label>Enter your message:
        <input type="text" value={msg()} onInput={e => setMsg(e.target.value)} />
        <button onClick={() => {
          const message = msg();
          const roomName = selectedRoom();
          if (!message || !roomName) return;
          ws.send(`${roomName}\n${message}`);
        }}>Send</button>
      </label>
    </div>
    }
  </div>;
}

export default App;
