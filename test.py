# Allocate ~5GB of memory
data = []
try:
    # Each iteration adds ~100MB
    for i in range(50):  # 50 * 100MB = ~5GB
        # Create a list of 100MB (25 million integers, ~4 bytes each)
        chunk = [0] * (25 * 1024 * 1024)
        data.append(chunk)
        print(f"Allocated {(i+1) * 100}MB")
except MemoryError:
    print("MemoryError: Hit the 3GB limit!")
except Exception as e:
    print(f"Error: {e}")