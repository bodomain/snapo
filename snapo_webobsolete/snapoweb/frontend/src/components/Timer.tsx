"use client";

import { useState, useEffect, useRef } from "react";
import axios from "axios";
import { Play, Pause, Square, CheckCircle, Briefcase, Coffee, RotateCcw } from "lucide-react";

export default function Timer({ onSessionLogged }: { onSessionLogged: () => void }) {
  const [mode, setMode] = useState<"work" | "shortBreak" | "longBreak" | "idle">("idle");
  const [timeLeft, setTimeLeft] = useState(25 * 60);
  const [isActive, setIsActive] = useState(false);

  // Configuration
  const [workMinutes, setWorkMinutes] = useState(25);
  const [shortBreakMinutes, setShortBreakMinutes] = useState(5);
  const [longBreakMinutes, setLongBreakMinutes] = useState(15);
  const [cycles, setCycles] = useState(4);
  const [currentCycle, setCurrentCycle] = useState(1);
  const [activity, setActivity] = useState("");

  const [notification, setNotification] = useState<{message: string, type: string} | null>(null);

  const audioRef = useRef<HTMLAudioElement | null>(null);

  const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

  useEffect(() => {
    // Attempt to play sound when timer reaches zero. Browser policies may block it without user interaction
    // We create the audio element once and keep it in ref
    audioRef.current = new Audio("/bell.wav"); // Requires copying bell.wav to public folder
  }, []);

  const playSound = () => {
    if (audioRef.current) {
      audioRef.current.play().catch(e => console.log("Audio play blocked by browser", e));
    }
  }

  useEffect(() => {
    let interval: NodeJS.Timeout | null = null;
    if (isActive && timeLeft > 0) {
      interval = setInterval(() => {
        setTimeLeft((prev) => prev - 1);
      }, 1000);
    } else if (isActive && timeLeft === 0) {
      handleTimerComplete();
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [isActive, timeLeft]);

  const handleTimerComplete = () => {
    playSound();
    setIsActive(false);

    if (mode === "work") {
      logSession(activity || "default", workMinutes);

      if (currentCycle % cycles === 0) {
        setMode("longBreak");
        setTimeLeft(longBreakMinutes * 60);
      } else {
        setMode("shortBreak");
        setTimeLeft(shortBreakMinutes * 60);
      }
    } else {
      // Break is over
      if (mode === "longBreak") {
        setCurrentCycle(1);
      } else {
        setCurrentCycle(prev => prev + 1);
      }
      setMode("work");
      setTimeLeft(workMinutes * 60);
    }
  };

  const startSession = () => {
    setMode("work");
    setTimeLeft(workMinutes * 60);
    setCurrentCycle(1);
    setIsActive(true);
  };

  const toggleTimer = () => {
    if (mode === "idle") startSession();
    else setIsActive(!isActive);
  };

  const stopTimer = () => {
    setIsActive(false);
    setMode("idle");
    setTimeLeft(workMinutes * 60);
    setCurrentCycle(1);
  };

  const submitSessionEarly = () => {
    if (mode === "work") {
      const elapsedMinutes = (workMinutes * 60 - timeLeft) / 60;
      logSession(activity || "default", Math.round(elapsedMinutes * 100) / 100);
    }
    stopTimer();
  };

  const logSession = async (actName: string, duration: number) => {
    try {
      await axios.post(`${API_URL}/api/sessions/log`, {
        activity: actName,
        duration_minutes: duration
      });
      showNotification(`Logged ${Math.round(duration)}m for '${actName}'`, "success");
      onSessionLogged();
    } catch (error) {
      console.error(error);
      showNotification("Failed to log session", "error");
    }
  };

  const showNotification = (msg: string, type: "success" | "error") => {
    setNotification({ message: msg, type });
    setTimeout(() => setNotification(null), 3000);
  };

  const formatTime = (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  };

  const progressPercentage = mode !== "idle"
    ? ((mode === "work" ? workMinutes * 60 : mode === "shortBreak" ? shortBreakMinutes * 60 : longBreakMinutes * 60) - timeLeft) /
      (mode === "work" ? workMinutes * 60 : mode === "shortBreak" ? shortBreakMinutes * 60 : longBreakMinutes * 60) * 100
    : 0;

  return (
    <div className="flex flex-col gap-8 w-full">
      {/* Settings Grid */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 p-4 bg-gray-50 border border-gray-200 rounded-xl">
        <label className="flex flex-col gap-1 text-sm text-gray-600">
          Activity Name
          <input
            type="text"
            placeholder="Focusing..."
            value={activity}
            onChange={e => setActivity(e.target.value)}
            disabled={mode !== "idle"}
            className="mt-1 p-2 rounded-lg bg-white border border-gray-300 text-black focus:outline-none focus:border-black transition-colors disabled:opacity-50"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm text-gray-600">
          Work (min)
          <input
            type="number"
            value={workMinutes}
            onChange={e => {setWorkMinutes(Number(e.target.value)); setTimeLeft(Number(e.target.value)*60)}}
            disabled={mode !== "idle"}
            className="mt-1 p-2 rounded-lg bg-white border border-gray-300 text-black focus:outline-none focus:border-black transition-colors disabled:opacity-50"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm text-gray-600">
          Break (min)
          <input
            type="number"
            value={shortBreakMinutes}
            onChange={e => setShortBreakMinutes(Number(e.target.value))}
            disabled={mode !== "idle"}
            className="mt-1 p-2 rounded-lg bg-white border border-gray-300 text-black focus:outline-none focus:border-black transition-colors disabled:opacity-50"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm text-gray-600">
          Cycles
          <input
            type="number"
            value={cycles}
            onChange={e => setCycles(Number(e.target.value))}
            disabled={mode !== "idle"}
            className="mt-1 p-2 rounded-lg bg-white border border-gray-300 text-black focus:outline-none focus:border-black transition-colors disabled:opacity-50"
          />
        </label>
      </div>

      {/* Main Timer Display */}
      <div className="flex flex-col items-center py-6">
        <div className="relative w-64 h-64 flex items-center justify-center">
          <svg className="absolute w-full h-full transform -rotate-90">
            <circle
              cx="128" cy="128" r="120"
              className="stroke-gray-300"
              strokeWidth="8" fill="none"
            />
            <circle
              cx="128" cy="128" r="120"
              className={`transition-all duration-1000 ease-linear stroke-black`}
              strokeWidth="10" fill="none"
              strokeDasharray={2 * Math.PI * 120}
              strokeDashoffset={2 * Math.PI * 120 * (1 - progressPercentage / 100)}
              strokeLinecap="round"
            />
          </svg>
          <div className="flex flex-col items-center justify-center z-10">
            <span className="text-6xl font-black tabular-nums tracking-tighter text-black">
              {formatTime(timeLeft)}
            </span>
            <span className={`text-sm tracking-widest uppercase font-semibold mt-2 text-black`}>
              {mode === "idle" ? "Ready" : mode === "work" ? "Focus" : "Break time!"}
            </span>
          </div>
        </div>
      </div>

      {/* Controls */}
      <div className="flex flex-wrap justify-center gap-4">
        <button
          onClick={toggleTimer}
          className={`flex items-center gap-2 px-8 py-4 rounded-xl font-bold transition-all transform hover:scale-105 active:scale-95 shadow-lg ${
            isActive ? "bg-gray-900 hover:bg-gray-800 text-white"
                     : "bg-black hover:bg-gray-900 text-white"
          }`}
        >
          {isActive ? <Pause className="w-5 h-5 fill-current" /> : <Play className="w-5 h-5 fill-current" />}
          {isActive ? "Pause" : mode === "idle" ? "Start Session" : "Resume"}
        </button>

        {mode !== "idle" && (
          <>
            <button
              onClick={stopTimer}
              className="flex items-center p-4 rounded-xl bg-gray-100 hover:bg-gray-200 text-gray-600 hover:text-gray-800 transition-colors border border-gray-200"
              title="Stop entire session"
            >
              <Square className="w-5 h-5 fill-current" />
            </button>
            {mode === "work" && (
              <button
                onClick={submitSessionEarly}
                className="flex items-center gap-2 px-6 py-4 rounded-xl bg-gray-100 hover:bg-gray-200 text-gray-800 transition-colors border border-gray-200 shadow-sm"
                title="Log current progress and stop"
              >
                <CheckCircle className="w-5 h-5" />
                Done Early
              </button>
            )}
            <div className="flex items-center px-4 rounded-xl bg-gray-100 border border-gray-200 text-gray-600 text-sm font-medium">
              Cycle {currentCycle} / {cycles}
            </div>
          </>
        )}
      </div>

      {notification && (
        <div className={`fixed bottom-8 right-8 px-6 py-3 rounded-xl shadow-lg z-50 animate-in slide-in-from-bottom border ${
          notification.type === "success"
            ? "bg-gray-900 border-gray-700 text-white"
            : "bg-gray-900 border-gray-700 text-red-400"
        }`}>
          {notification.message}
        </div>
      )}
    </div>
  );
}
