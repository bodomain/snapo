"use client";

import { useEffect, useState } from "react";
import axios from "axios";
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend
} from "recharts";
import { Activity } from "lucide-react";

interface StatEntry {
  date: string;
  activity: string;
  duration_minutes: number;
}

export default function Stats() {
  const [data, setData] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [activitiesList, setActivitiesList] = useState<string[]>([]);
  
  const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

  useEffect(() => {
    fetchStats();
  }, []);

  const fetchStats = async () => {
    try {
      const resp = await axios.get<StatEntry[]>(`${API_URL}/api/stats`);
      
      // Transform data for Recharts stacked bar
      // Recharts expects [{ date: "2023-11-01", "coding": 120, "reading": 45 }, ...]
      const groupedData: Record<string, any> = {};
      const activities = new Set<string>();

      resp.data.forEach(entry => {
        activities.add(entry.activity);
        if (!groupedData[entry.date]) {
          groupedData[entry.date] = { date: entry.date, total: 0 };
        }
        
        if (!groupedData[entry.date][entry.activity]) {
          groupedData[entry.date][entry.activity] = 0;
        }
        groupedData[entry.date][entry.activity] += entry.duration_minutes;
        groupedData[entry.date].total += entry.duration_minutes;
      });

      // Sort by date strings safely
      const finalData = Object.values(groupedData).sort((a,b) => {
          return new Date(a.date).getTime() - new Date(b.date).getTime();
      });

      setActivitiesList(Array.from(activities));
      setData(finalData);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  // Color palette for activities
  const colors = ["#818cf8", "#34d399", "#f472b6", "#fbbf24", "#60a5fa", "#a78bfa", "#f87171"];

  if (loading) return (
    <div className="h-full w-full flex items-center justify-center text-zinc-500 animate-pulse">
      Loading chart...
    </div>
  );
  
  if (data.length === 0) return (
    <div className="h-full w-full flex flex-col items-center justify-center text-zinc-500 gap-3">
      <div className="w-16 h-16 rounded-full bg-zinc-800/50 flex items-center justify-center">
        <Activity className="w-8 h-8 opacity-50" />
      </div>
      <p>No activity data available yet.</p>
    </div>
  );

  return (
    <ResponsiveContainer width="100%" height="100%">
      <BarChart
        data={data}
        margin={{ top: 10, right: 10, left: -20, bottom: 0 }}
      >
        <CartesianGrid strokeDasharray="3 3" stroke="#27272a" vertical={false} />
        <XAxis 
          dataKey="date" 
          stroke="#71717a" 
          fontSize={12} 
          tickLine={false}
          axisLine={false}
          dy={10}
        />
        <YAxis 
          stroke="#71717a" 
          fontSize={12}
          tickLine={false}
          axisLine={false}
          tickFormatter={(val) => `${val}m`}
        />
        <Tooltip 
          cursor={{fill: '#27272a', opacity: 0.4}}
          contentStyle={{ backgroundColor: '#18181b', borderColor: '#3f3f46', borderRadius: '12px', color: '#fff' }}
          itemStyle={{ color: '#e4e4e7' }}
          formatter={(value: number | undefined) => [`${Math.round(value || 0)} mins`, ""]}
        />
        <Legend wrapperStyle={{ paddingTop: "20px", fontSize: '12px', color: '#a1a1aa' }} iconType="circle" />
        {activitiesList.map((activity, idx) => (
          <Bar 
            key={activity} 
            dataKey={activity} 
            stackId="a" 
            fill={colors[idx % colors.length]} 
            radius={
                // Top radius only on the topmost bar block
                idx === activitiesList.length - 1 ? [4, 4, 0, 0] : [0,0,0,0]
            }
          />
        ))}
      </BarChart>
    </ResponsiveContainer>
  );
}
