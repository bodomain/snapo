import pandas as pd
from wetterdienst.provider.dwd.observation import DwdObservationRequest

def fetch_daily_temperature(station_id: str):
    # 1. Define the request
    # We request 'kl' (climate summary, which includes temperature)
    # at a 'daily' resolution for the 'historical' period.
    request = DwdObservationRequest(
        parameters=[("daily", "kl")],
        periods=["historical"]
    ).filter_by_station_id(station_id=(station_id,))

    # 2. Fetch the data
    # .all() fetches data for all stations in the request (just one here)
    # .df() returns a Polars DataFrame (the default in recent versions)
    # We convert it to a Pandas DataFrame for familiarity.
    print(f"Fetching data for station {station_id}...")
    df_polars = request.values.all().df
    df = df_polars.to_pandas()

    # 3. Clean up the data
    # The 'kl' parameter returns many columns (wind, rain, etc.). 
    # Let's filter just for the daily mean temperature at 2m height ('temperature_air_mean_200')
    if df.empty:
        print("No data found for this station.")
        return df

    # We filter where the 'parameter' column is 'temperature_air_mean_2m'
    temp_df = df[df['parameter'] == 'temperature_air_mean_2m'].copy()
    
    # Keep only the relevant columns and sort by date
    temp_df = temp_df[['station_id', 'date', 'value']]
    temp_df = temp_df.rename(columns={'value': 'temperature_celsius'})
    temp_df = temp_df.sort_values(by='date')

    return temp_df

if __name__ == "__main__":
    # Station ID "00433" corresponds to Berlin-Tempelhof
    # Note: Station IDs are always strings, often with leading zeros.
    station_id = "00433" 
    
    historical_temps = fetch_daily_temperature(station_id)
    
    print("\nSample Data (Berlin-Tempelhof):")
    print(historical_temps.head())
    print(historical_temps.tail())
