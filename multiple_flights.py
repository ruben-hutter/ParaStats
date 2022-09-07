import os

from single_flight import SingleFlight


class MultipleFlights:
    def __init__(self, directory_path):
        self.flights = []
        self.number_of_flights = 0
        self.mean_takeoff_altitude = 0
        self.mean_max_altitude = 0
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
        print("===== Mean of all flights =====")

    def calculate_mean(self):
        self.mean_takeoff_altitude /= self.number_of_flights
        self.mean_max_altitude /= self.number_of_flights
        self.mean_max_integrated_climb /= self.number_of_flights
        self.mean_max_integrated_sink /= self.number_of_flights

    def _calculate_mean(self, flight):
        self.number_of_flights += 1
        self.mean_takeoff_altitude += flight.sens_gps_mean[0]
        self.mean_max_altitude += flight.max_altitude
        self.mean_max_integrated_climb += flight.max_integrated_climb
        self.mean_max_integrated_sink += flight.max_integrated_sink
