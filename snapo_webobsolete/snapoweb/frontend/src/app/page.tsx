"use client";

import { useState } from "react";
import TimerComponent from "@/components/Timer";
import StatsComponent from "@/components/Stats";

export default function Home() {
  const [statsKey, setStatsKey] = useState(0);

  const handleSessionLogged = () => {
    // Increment the key to force re-render/refetch of stats
    setStatsKey((prev) => prev + 1);
  };

  return (
    <div className="space-y-8 animate-in fade-in zoom-in-95 duration-500">
      <header className="mb-8">
        <h2 className="text-3xl font-bold tracking-tight text-black mb-2">Welcome Back</h2>
        <p className="text-gray-600">Ready to crush some goals today?</p>
      </header>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div className="lg:col-span-8 flex flex-col gap-6">
          <div className="bg-gray-50 border border-gray-200 rounded-3xl p-6 md:p-8 shadow-sm overflow-hidden relative">
            <h3 className="text-xl font-semibold mb-6 flex items-center gap-2">
              <span className="w-1.5 h-6 rounded-full bg-black inline-block" />
              Focus Timer
            </h3>
            <TimerComponent onSessionLogged={handleSessionLogged} />
          </div>

          <div className="border border-gray-200/0 rounded-3xl md:p-0 shadow-sm">
            {/* Added for spacing parity */}
          </div>
        </div>

        <div className="lg:col-span-4 h-full flex flex-col gap-6">
          <div className="bg-gray-50 border border-gray-200 rounded-3xl p-6 shadow-sm flex-grow h-full min-h-[400px]">
            <h3 className="text-xl font-semibold mb-6 flex items-center gap-2">
              <span className="w-1.5 h-6 rounded-full bg-black inline-block" />
              Your Activity
            </h3>
            <div className="h-[calc(100%-3rem)] min-h-[300px]">
              <StatsComponent key={statsKey} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
