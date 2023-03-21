import datetime as dt
import os

from constants import Constants
from single_flight import SingleFlight


def divider(message):
    msg_len = len(message)
    total_div_symbl = Constants.DIVIDER_LEN - msg_len
    one_side_divider = "".join(["=" for _ in range(total_div_symbl // 2 - 1)])
    divider_message = " ".join([one_side_divider, message, one_side_divider])
    if len(divider_message) < Constants.DIVIDER_LEN:
        divider_message += "="
    return divider_message


def round_seconds(obj: dt.timedelta) -> dt.timedelta:
    if obj.microseconds >= 500_000:
        obj += dt.timedelta(seconds=1)
    return dt.timedelta(seconds=obj.seconds)


def days_to_hours(obj: dt.timedelta) -> str:
    d = obj.days
    s = obj.seconds
    hours, remainder = divmod(s, 3600)
    minutes, seconds = divmod(remainder, 60)
    hours += d * 24
    return f"{hours:02}:{minutes:02}:{seconds:02}"


class MultipleFlights:
    def __init__(self, directory_path):
        self.flights = []
        self.number_of_flights = 0
        self.mean_flight_duration = dt.timedelta()
        self.total_flight_time = dt.timedelta()
        self.absolute_max_altitude = 0
        self.mean_max_integrated_climb = 0
        self.mean_max_integrated_sink = 0
        # initialize obj
        self.initialize_flights(directory_path)

    def initialize_flights(self, directory_path):
        # iterate over directory files
        for filename in sorted(os.listdir(directory_path)):
            # create single flight obj for every file
            self.flights.append(SingleFlight(os.path.join(directory_path, filename)))

    def print_stats(self):
        for flight in self.flights:
            # sum up data in MultipleFlights obj
            # in calculate_mean() just divide to num of flights
            # (method to iterate only one time over flights)
            self._calculate_mean(flight)
            flight.print_stats()
        self.calculate_mean()
        print(divider("Mean of all flights"))
        print(f"Flight duration:\t{self.mean_flight_duration}")
        print(f"Total air time:\t{self.total_flight_time}")
        print(f"Max altitude:\t{self.absolute_max_altitude:.1f} m")
        print(f"Max integrated climb rate ({Constants.INTEGRATION_TIME} s):\t{self.mean_max_integrated_climb:.1f} m/s")
        print(f"Max integrated sink rate ({Constants.INTEGRATION_TIME} s):\t{self.mean_max_integrated_sink:.1f} m/s")

    def calculate_mean(self):
        self.mean_flight_duration /= self.number_of_flights
        self.mean_flight_duration = round_seconds(self.mean_flight_duration)
        self.total_flight_time = days_to_hours(self.total_flight_time)
        self.mean_max_integrated_climb /= self.number_of_flights
        self.mean_max_integrated_sink /= self.number_of_flights

    def _calculate_mean(self, flight):
        self.number_of_flights += 1
        self.mean_flight_duration += flight.flight_duration
        self.total_flight_time += flight.flight_duration
        if self.absolute_max_altitude < flight.max_altitude:
            self.absolute_max_altitude = flight.max_altitude
        self.mean_max_integrated_climb += flight.max_integrated_climb
        self.mean_max_integrated_sink += flight.max_integrated_sink
