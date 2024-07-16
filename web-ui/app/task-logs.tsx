"use client";

import { useEffect, useRef, useState } from "react";
import Ansi from "ansi-to-react";
import { useInterval } from "usehooks-ts";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export default function TaskLogs() {
  const [logs, setLogs] = useState<{ [s: string]: string }>({});
  const [messagesSeen, setMessagesSeen] = useState({});
  const [connected, setConnected] = useState(false);
  const socketRef = useRef<WebSocket | null>(null);
  useInterval(() => {
    if (
      !socketRef.current ||
      socketRef.current.readyState === WebSocket.CLOSED
    ) {
      socketRef.current = new WebSocket("ws://127.0.0.1:1337/ws");
      const textDecoder = new TextDecoder();

      socketRef.current.onerror = (event) => {
        console.log("error");
      };

      socketRef.current.onmessage = (event) => {
        const data = JSON.parse(event.data);
        const byteArray = new Uint8Array(data.payload.output);
        const rawLogs = textDecoder.decode(byteArray);
        if (data.id in messagesSeen) {
          return;
        }
        setMessagesSeen((messagesSeen) => ({
          ...messagesSeen,
          [data.id]: true,
        }));
        console.log(data);
        if (data.type === "TaskOutput") {
          setLogs((logs) => {
            return {
              ...logs,
              [data.payload.task]: (logs[data.payload.task] ?? "") + rawLogs,
            };
          });
        }
        socketRef.current?.send(
          JSON.stringify({
            type: "Ack",
            payload: { id: data.id },
          })
        );
      };

      socketRef.current.onopen = () => {
        socketRef.current?.send(
          JSON.stringify({
            type: "CatchUp",
            payload: { start_id: 0 },
          })
        );
        console.log("connected!");
        setConnected(true);
      };
    }
  }, 1000);

  useEffect(() => {
    return () => {
      if (socketRef.current) {
        socketRef.current.close();
      }
    };
  });

  return (
    <div className="bg-gray-900 min-h-screen">
      <div className="flex justify-between bg-gradient-to-r p-4 from-indigo-500 via-purple-500 to-pink-500">
        <h1 className="text-white font-semibold">TURBO STUDIO</h1>
      </div>
      <div>
        {!connected && <div className="text-white p-4">Connecting...</div>}

        <Tabs
          orientation="vertical"
          defaultValue="account"
          className="w-[400px]"
        >
          <TabsList>
            {Object.entries(logs).map(([taskName]) => (
              <TabsTrigger value={taskName} key={taskName}>
                {taskName}
              </TabsTrigger>
            ))}
          </TabsList>
          {Object.entries(logs).map(([taskName, taskLogs]) => (
            <TabsContent value={taskName} key={taskName}>
              <Ansi className="whitespace-pre-wrap text-white p-5">
                {taskLogs}
              </Ansi>
            </TabsContent>
          ))}
        </Tabs>
      </div>
    </div>
  );
}
